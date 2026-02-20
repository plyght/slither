use dashmap::DashMap;
use reqwest::Client;
use std::sync::Arc;

#[derive(Debug, Clone)]
struct RobotsRule {
    allow: bool,
    path: String,
}

#[derive(Debug, Clone)]
pub struct RobotsInfo {
    rules: Vec<RobotsRule>,
    #[allow(dead_code)]
    pub crawl_delay_secs: Option<u64>,
    allowed_all: bool,
}

impl RobotsInfo {
    fn allow_all() -> Self {
        Self {
            rules: vec![],
            crawl_delay_secs: None,
            allowed_all: true,
        }
    }

    pub fn is_allowed(&self, path: &str) -> bool {
        if self.allowed_all {
            return true;
        }

        let mut best_len: usize = 0;
        let mut best_allow = true;

        for rule in &self.rules {
            if path.starts_with(rule.path.as_str()) && rule.path.len() >= best_len {
                best_len = rule.path.len();
                best_allow = rule.allow;
            }
        }

        best_allow
    }
}

fn parse_robots_txt(text: &str, user_agent: &str) -> RobotsInfo {
    let ua_lower = user_agent.to_lowercase();
    let mut in_matching_section = false;
    let mut rules: Vec<RobotsRule> = Vec::new();
    let mut crawl_delay: Option<u64> = None;

    for line in text.lines() {
        let line = line.trim();

        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        let (key, value) = match line.split_once(':') {
            Some((k, v)) => (k.trim().to_lowercase(), v.trim()),
            None => continue,
        };

        match key.as_str() {
            "user-agent" => {
                let val_lower = value.to_lowercase();
                in_matching_section = val_lower == "*"
                    || ua_lower.contains(val_lower.as_str())
                    || val_lower.contains(ua_lower.as_str());
            }
            "disallow" if in_matching_section => {
                if !value.is_empty() {
                    rules.push(RobotsRule {
                        allow: false,
                        path: value.to_string(),
                    });
                }
            }
            "allow" if in_matching_section => {
                if !value.is_empty() {
                    rules.push(RobotsRule {
                        allow: true,
                        path: value.to_string(),
                    });
                }
            }
            "crawl-delay" if in_matching_section => {
                if let Ok(d) = value.parse::<u64>() {
                    crawl_delay = Some(d);
                }
            }
            _ => {}
        }
    }

    RobotsInfo {
        rules,
        crawl_delay_secs: crawl_delay,
        allowed_all: false,
    }
}

pub struct RobotsCache {
    cache: DashMap<String, Arc<RobotsInfo>>,
    client: Client,
    user_agent: String,
}

impl RobotsCache {
    pub fn new(client: Client, user_agent: String) -> Self {
        Self {
            cache: DashMap::new(),
            client,
            user_agent,
        }
    }

    pub async fn is_allowed(&self, url: &str) -> bool {
        let parsed = match url::Url::parse(url) {
            Ok(u) => u,
            Err(_) => return true,
        };

        let domain = match parsed.host_str() {
            Some(h) => h.to_string(),
            None => return true,
        };

        let robots_info = self.get_or_fetch(&domain, &parsed).await;
        let path = parsed.path();
        robots_info.is_allowed(path)
    }

    async fn get_or_fetch(&self, domain: &str, parsed_url: &url::Url) -> Arc<RobotsInfo> {
        if let Some(info) = self.cache.get(domain) {
            return info.clone();
        }

        let robots_url = format!("{}://{}/robots.txt", parsed_url.scheme(), domain);

        let info = match self.client.get(&robots_url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.text().await {
                Ok(text) => parse_robots_txt(&text, &self.user_agent),
                Err(_) => RobotsInfo::allow_all(),
            },
            _ => RobotsInfo::allow_all(),
        };

        let info = Arc::new(info);
        self.cache.insert(domain.to_string(), info.clone());
        info
    }
}

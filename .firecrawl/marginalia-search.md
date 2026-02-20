- 🏠 [Marginalia](https://www.marginalia.nu/)
- 📁 [Marginalia Search](https://www.marginalia.nu/marginalia-search/)

# Marginalia Search

**This information is outdated**

If you are looking for the search engine itself, visit [https://marginalia-search.com](https://marginalia-search.com/).

The marginalia search project information page now lives over at [about.marginalia-search.com](https://about.marginalia-search.com/).

Information relating to the Marginalia Search project.

Marginalia Search is an independent DIY search engine that focuses on non-commercial content, and attempts to show you sites you perhaps weren’t aware of in favor of the sort of sites you probably already knew existed.

| URL: | 🌎 [https://search.marginalia.nu/](https://search.marginalia.nu/) |
| Git: | 🌎 [https://git.marginalia.nu/](https://git.marginalia.nu/) |

You may also be interested in the 🏷️ [search-engine](https://www.marginalia.nu/tags/search-engine) tag.

## Documents

| Name | Date |
| --- | --- |
| 📁 [../](https://www.marginalia.nu/) | 2026-02-19 |
| 📄<br>[FAQ](https://www.marginalia.nu/marginalia-search/faq/) | 2023-03-28 |
| 📄<br>[API](https://www.marginalia.nu/marginalia-search/api/) | 2023-03-23 |
| 📄<br>[About Marginalia Search](https://www.marginalia.nu/marginalia-search/about/) | 2022-12-23 |
| 📄<br>[For Webmasters](https://www.marginalia.nu/marginalia-search/for-webmasters/) | 2022-10-28 |
| 📄<br>[Privacy Considerations](https://www.marginalia.nu/marginalia-search/privacy/) | 2022-09-22 |
| 📄<br>[Donate To This Project](https://www.marginalia.nu/marginalia-search/supporting/) | 2022-09-05 |

## Recent Posts in 🏷️ [search-engine](https://www.marginalia.nu/tags/search-engine)

#### 2026-02-13 [Index Compression, Query Execution Improvements](https://www.marginalia.nu/log/a_131_index_compression/)

The Marginalia Search index has recently seen some design tweaks to make it perform better, primarily the introduction of postings list compression.
Last year, the index was partially re-implemented with SSDs in mind. This was largely a success, but left some lingering issues with tail latencies that sometimes weren’t what they needed to be.
To ensure predictable execution times, the query execution is provided a timeout value, after which it will wrap up and return the best results it’s found.

#### 2026-01-31 [Trust in Ranking](https://www.marginalia.nu/log/a_130_trust_in_ranking/)

The Marginalia Search default ranking algorithm recently saw a fairly radical improvement, due to a new domain trust system that drastically reduces the number of content farm results, as long as there are human results it usually finds them across all the usual test queries.
Recently fixing a few bugs that made the search engine work more correctly had the unexpected and undesired side-effect of also making it surface more search engine spam and content farm-type results.

#### 2025-12-08 [New Search Filtering in Web and API](https://www.marginalia.nu/log/a_127_index_filtering/)

The search engine recently exposed a fair number of new tools for custom filtering to the API consumers and users of the new UI.
This was originally going to be an incredibly chaotic update, both annuncing the new features and doing a technical walkthrough of the changes but that ambition turned out a bit too chaotic, so let’s split them up and focus on the feature announcement bit today.
New Search Filtering GUI It’s now possible to define a custom filter in the GUI, on the marginalia-search.

#### 2025-10-06 [Language Support for Marginalia Search](https://www.marginalia.nu/log/a_126_multilingual/)

One of the big ambitions for the search engine this year has been to enable searching in more languages than English, and a pilot project for this has just been completed, allowing experimental support for German, French and Swedish.
These changes are now live for testing, but with an extremely small corpus of documents.
As the search engine has been up to this point built with English in mind, some anglo-centric assumptions made it into its code.

#### 2025-08-16 [Faster Index I/O with NVMe SSDs](https://www.marginalia.nu/log/a_123_index_io/)

The Marginalia Search index has been partially rewritten to perform much better, using new data structures designed to make better use of modern hardware. This post will cover the new design, and will also touch upon some of the unexpected and unintuitive performance characteristics of NVMe SSDs when it comes to read sizes.
The index is already fairly large, but can sometimes feel smaller than it is, and paradoxically, query performance is a big part of why.

#### 2025-06-17 [Finding Dead Websites](https://www.marginalia.nu/log/a_122_dead_websites/)

As some of the work planned for Marginalia Search this year has been progressing a bit faster than anticipated, there was time to implement an unplanned change.
This post details the implementation of a system for detecting when servers are online, to avoid serving dead links and improve data quality, and for detecting when websites have significant changes including ownership transfers and parking.
Table Of Contents Feature Rationale Data Representation Live Data Event Data Change Detection Details Availability Detection Ownership Changes DNS Implementation Hurdles Scheduling Certificate Validation Conclusions Feature Rationale Availability detection is useful not just for filtering out dead links in the search results, but for informing the crawler that it should stop trying to reach a dead domain, as well as a host of other things.

#### 2025-05-29 [Profiling Websites](https://www.marginalia.nu/log/a_121_profiling_websites/)

The most recent change to the search engine is a system that profiles websites based on their rendered DOM. The goal is identifying advertisements, trackers, nuisance popovers, and similar elements.
The search engine already tries to do this, but isn’t very good at it because it’s only looking at static code.
It turns out to be somewhat difficult to determine what a website that has non-trivial javascript will look like based its source code alone, as this would require us to among other things solve the halting problem.

#### 2025-05-13 [PDF to Text, a challenging problem](https://www.marginalia.nu/log/a_119_pdf/)

The search engine has recently gained the ability to index the PDF file format. The change will deploy over a few months.
Extracting text information from PDFs is a significantly bigger challenge than it might seem. The crux of the problem is that the file format isn’t a text format at all, but a graphical format.
It doesn’t have text in the way you might think of it, but more of a mapping of glyphs to coordinates on “paper”.

#### 2025-03-27 [Crawl Order and Disorder](https://www.marginalia.nu/log/a_117_crawl_order/)

A problem the search engine’s crawler has struggled with for some time is that it takes a fairly long time to finish up, usually spending several days wrapping up the final few domains.
This has been actualized recently, since the migration to slop crawl data has dropped memory requirements of the crawler by something like 80%, and as such I’ve been able to increase the number of crawling tasks, which has led to a bizarre case where 99.

#### 2025-03-25 [Marginalia Search receives second nlnet grant](https://www.marginalia.nu/log/a_116_grant_2.0/)

I’m happy and grateful to announce that the Marginalia Search project has been accepted for a second nlnet grant.
All the details are not yet finalized, but tentatively the grant will go toward addressing most of the items in the project roadmap for 2025.
I’ve already been working full time on the project since summer 2023, and this grant secures additional development time, and extends the runway to a comfortable degree.

#### 2025-03-03 [Marginalia Search: 4 Years](https://www.marginalia.nu/log/a_114_4_years/)

This update is a few days late, the canonical birth date of the project is Feb 26.
It has been another year of Marginalia Search. The project is still ongoing, still my full time job, although the project is entering a somewhat more mature phase of development, most of the big pieces are in place and do a decent job at what they do.
The roadmap for the project is available on GitHub.

#### 2024-12-26 [RSS Feeds and Real Time Crawling](https://www.marginalia.nu/log/a_113_rtc/)

A while back an update went live that, with some caveats, changes the time it takes for an update on a website to reflect in the search engine index from up to 2 months to 1-2 days. Conditions being if the website has an RSS or Atom feed.
The big crawl job takes about two months, and is run partition by partition, meaning there’s typically a slice of the index that is two months stale at any given point in time.

#### 2024-11-05 [Notes on binary soup](https://www.marginalia.nu/log/a_112_slop_ideas/)

I recently put together a small library called Slop, for intermediate on-disk data representation for the search engine, replacing a few ad-hoc formats I had in place before.
This post isn’t so much an attempt to convince anyone else to use this library, as it makes trade-offs catering to a fairly niche use case, but to explore some of its design ideas, as it all came together very nicely, in the hopes that other libraries can draw ideas from it.

#### 2024-09-30 [Phrase Matching in Marginalia Search](https://www.marginalia.nu/log/a_111_phrase_matching/)

Marginalia Search now properly supports phrase matching. This not only permits a more robust implementation of quoted search queries, but also helps promote results where the search terms occur in the document exactly in the same order as they do in the query.
This is a write-up about implementing this change. This is going to be a relatively long post, as it represents about 4 months of work.
I’m also happy and grateful to announce that the nlnet people reached out after the run of the grant was over and asked me if I had more work in the pipe, and agreed to fund this change as well!

[marginalia.nu](https://www.marginalia.nu/) © 2026
Viktor Löfgren <kontakt@marginalia.nu>

Consider [donating](https://www.marginalia.nu/marginalia-search/supporting/) to support the development effort of Marginalia Search and
the other services!
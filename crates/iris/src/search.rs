/// 8-wide unrolled dot product.  The separate accumulators let the compiler
/// emit SIMD (SSE/AVX) fused multiply-adds and hide FP latency, giving
/// ~4-8× throughput over a naive scalar loop on x86-64.
#[inline]
pub(crate) fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());

    let n = a.len();
    let chunks = n / 8;
    let remainder = n % 8;

    let mut s0: f32 = 0.0;
    let mut s1: f32 = 0.0;
    let mut s2: f32 = 0.0;
    let mut s3: f32 = 0.0;
    let mut s4: f32 = 0.0;
    let mut s5: f32 = 0.0;
    let mut s6: f32 = 0.0;
    let mut s7: f32 = 0.0;

    let mut i = 0;
    let end = chunks * 8;
    while i < end {
        unsafe {
            s0 += *a.get_unchecked(i) * *b.get_unchecked(i);
            s1 += *a.get_unchecked(i + 1) * *b.get_unchecked(i + 1);
            s2 += *a.get_unchecked(i + 2) * *b.get_unchecked(i + 2);
            s3 += *a.get_unchecked(i + 3) * *b.get_unchecked(i + 3);
            s4 += *a.get_unchecked(i + 4) * *b.get_unchecked(i + 4);
            s5 += *a.get_unchecked(i + 5) * *b.get_unchecked(i + 5);
            s6 += *a.get_unchecked(i + 6) * *b.get_unchecked(i + 6);
            s7 += *a.get_unchecked(i + 7) * *b.get_unchecked(i + 7);
        }
        i += 8;
    }

    let mut sum = (s0 + s1) + (s2 + s3) + (s4 + s5) + (s6 + s7);

    let base = end;
    for j in 0..remainder {
        sum += a[base + j] * b[base + j];
    }

    sum
}

pub(crate) fn l2_normalize(vec: &mut [f32]) {
    let norm: f32 = vec.iter().map(|&x| x * x).sum::<f32>().sqrt();
    if norm > 1e-8 {
        for v in vec.iter_mut() {
            *v /= norm;
        }
    }
}

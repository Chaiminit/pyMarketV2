use rand::distributions::{Distribution, Standard};
use rand::Rng;
use rust_decimal::Decimal;
use std::f64::consts::E;

/// Sigmoid function
pub fn sigmoid(x: f64) -> f64 {
    if x > 10.0 {
        1.0
    } else if x < -10.0 {
        0.0
    } else {
        1.0 / (1.0 + E.powf(-x))
    }
}

/// Chip distribution for trading volume calculation
pub struct ChipDistribution {
    a: f64,
    m: f64,
    alpha: f64,
    lambda: f64,
    left_cdf: f64,
    inv_a: f64,
}

impl ChipDistribution {
    pub fn new(a: f64, m: f64, alpha: Option<f64>) -> Self {
        if a <= 0.0 {
            panic!("a must be positive");
        }
        if m <= 0.0 {
            panic!("m must be positive");
        }

        let alpha = alpha.unwrap_or_else(|| (1.0f64.max(m * a)) + 1.0);

        if alpha <= 0.0 {
            panic!("alpha must be positive");
        }

        let left_cdf = m * a / (alpha + 1.0);

        if !(0.0..1.0).contains(&left_cdf) {
            panic!(
                "Parameters result in left CDF L={} not in (0,1). Adjust alpha or (a,m)",
                left_cdf
            );
        }

        let lambda = m / (1.0 - left_cdf);
        let inv_a = 1.0 / a;

        Self {
            a,
            m,
            alpha,
            lambda,
            left_cdf,
            inv_a,
        }
    }

    /// Calculate PDF value
    pub fn pdf(&self, x: f64) -> f64 {
        let x = x.max(0.0);
        let t = x * self.inv_a;

        if x <= self.a {
            self.m * t.powf(self.alpha)
        } else {
            self.m * (-self.lambda * (x - self.a)).exp()
        }
    }

    /// Sample from the distribution
    pub fn sample<R: Rng>(&self, rng: &mut R) -> f64 {
        let u: f64 = rng.sample(Standard);

        if u < self.left_cdf {
            // Left side
            let ratio = (u / self.left_cdf).min(1.0);
            self.a * ratio.powf(1.0 / (self.alpha + 1.0))
        } else {
            // Right side
            let numerator = 1.0 - u;
            let denominator = 1.0 - self.left_cdf;
            let eps = f64::MIN_POSITIVE;
            let frac = (numerator / denominator).max(eps);
            self.a - (1.0 / self.lambda) * frac.ln()
        }
    }
}

/// Convert Decimal to f64
pub fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse().unwrap_or(0.0)
}

/// Convert f64 to Decimal
pub fn f64_to_decimal(f: f64) -> Decimal {
    Decimal::try_from(f).unwrap_or(Decimal::ZERO)
}

//! Tiny tween engine replacing GSAP.

#[derive(Clone, Copy, Debug)]
pub enum Ease {
    /// GSAP "power2.inOut" (cubic)
    Power2InOut,
    /// GSAP "power3.inOut" (quartic)
    Power3InOut,
    /// GSAP "back.out(1.5)"
    BackOut,
}

impl Ease {
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Power2InOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Ease::Power3InOut => {
                if t < 0.5 {
                    8.0 * t.powi(4)
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(4) / 2.0
                }
            }
            Ease::BackOut => {
                let c1 = 1.5;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
        }
    }
}

/// Animates a scalar from `from` to `to`.
#[derive(Clone, Copy, Debug)]
pub struct Tween {
    from: f64,
    to: f64,
    start: f64,
    duration: f64,
    ease: Ease,
}

impl Tween {
    pub fn fixed(v: f64) -> Self {
        Tween { from: v, to: v, start: 0.0, duration: 0.0, ease: Ease::Power2InOut }
    }

    pub fn value(&self, now: f64) -> f64 {
        if self.duration <= 0.0 || now >= self.start + self.duration {
            return self.to;
        }
        let t = ((now - self.start) / self.duration).max(0.0);
        self.from + (self.to - self.from) * self.ease.apply(t)
    }

    pub fn target(&self) -> f64 {
        self.to
    }

    /// Retarget, starting from the value currently on screen (no jumps on interruption).
    pub fn retarget(&mut self, to: f64, now: f64, duration: f64, ease: Ease) {
        self.from = self.value(now);
        self.to = to;
        self.start = now;
        self.duration = duration;
        self.ease = ease;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_hit_endpoints() {
        for e in [Ease::Power2InOut, Ease::Power3InOut, Ease::BackOut] {
            assert!(e.apply(0.0).abs() < 1e-9);
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn tween_progresses() {
        let mut t = Tween::fixed(0.0);
        t.retarget(10.0, 0.0, 1.0, Ease::Power2InOut);
        assert!((t.value(0.5) - 5.0).abs() < 1e-9);
        assert_eq!(t.value(2.0), 10.0);
    }
}

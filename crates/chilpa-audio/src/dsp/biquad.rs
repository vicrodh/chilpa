//! RBJ cookbook biquads, evaluated in double precision with independent state.
use super::{Filter, Shape};

#[derive(Clone, Copy)]
pub(super) struct Biquad { b: [f64; 3], a: [f64; 2], z: [f64; 2] }
impl Biquad {
    pub(super) fn new(f: &Filter, rate: u32) -> Self {
        // A band above Nyquist cannot be represented. I leave it out rather
        // than move a treble boost into the audible range of a low-rate file.
        if f.frequency >= f64::from(rate) * 0.49 { return Self::identity(); }
        let w = 2.0 * std::f64::consts::PI * f.frequency / f64::from(rate);
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * f.q);
        let a = 10_f64.powf(f.gain_db / 40.0);
        let root = 2.0 * a.sqrt() * alpha;
        let (b, d) = match f.shape {
            Shape::Peak => ([1.0 + alpha * a, -2.0*c, 1.0 - alpha*a], [1.0 + alpha/a, -2.0*c, 1.0 - alpha/a]),
            Shape::LowPass => ([(1.0-c)/2.0, 1.0-c, (1.0-c)/2.0], [1.0+alpha, -2.0*c, 1.0-alpha]),
            Shape::HighPass => ([(1.0+c)/2.0, -1.0-c, (1.0+c)/2.0], [1.0+alpha, -2.0*c, 1.0-alpha]),
            Shape::Notch => ([1.0, -2.0*c, 1.0], [1.0+alpha, -2.0*c, 1.0-alpha]),
            Shape::LowShelf => ([a*((a+1.0)-(a-1.0)*c+root), 2.0*a*((a-1.0)-(a+1.0)*c), a*((a+1.0)-(a-1.0)*c-root)],
                [(a+1.0)+(a-1.0)*c+root, -2.0*((a-1.0)+(a+1.0)*c), (a+1.0)+(a-1.0)*c-root]),
            Shape::HighShelf => ([a*((a+1.0)+(a-1.0)*c+root), -2.0*a*((a-1.0)+(a+1.0)*c), a*((a+1.0)+(a-1.0)*c-root)],
                [(a+1.0)-(a-1.0)*c+root, 2.0*((a-1.0)-(a+1.0)*c), (a+1.0)-(a-1.0)*c-root]),
        };
        Self { b: b.map(|v| v/d[0]), a: [d[1]/d[0], d[2]/d[0]], z: [0.0; 2] }
    }
    fn identity() -> Self { Self { b: [1.0, 0.0, 0.0], a: [0.0; 2], z: [0.0; 2] } }
    pub(super) fn magnitude_db(&self, frequency: f64, rate: u32) -> f64 {
        let w = 2.0 * std::f64::consts::PI * frequency / f64::from(rate);
        let (s, c) = w.sin_cos();
        let (s2, c2) = (2.0*w).sin_cos();
        let numerator = (self.b[0]+self.b[1]*c+self.b[2]*c2).powi(2) + (self.b[1]*s+self.b[2]*s2).powi(2);
        let denominator = (1.0+self.a[0]*c+self.a[1]*c2).powi(2) + (self.a[0]*s+self.a[1]*s2).powi(2);
        10.0 * (numerator.max(1e-30)/denominator.max(1e-30)).log10()
    }
    pub(super) fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0]*x + self.z[0];
        self.z[0] = self.b[1]*x - self.a[0]*y + self.z[1];
        self.z[1] = self.b[2]*x - self.a[1]*y;
        for z in &mut self.z { if z.abs() < 1e-30 { *z = 0.0; } }
        y
    }
}

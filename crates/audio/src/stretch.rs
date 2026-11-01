use signalsmith_stretch::Stretch;

pub(crate) struct Stretcher {
    inner: Stretch,
}

impl Stretcher {
    pub fn new(rate: u32) -> Self {
        Self {
            inner: Stretch::preset_default(2, rate),
        }
    }

    pub fn input_latency(&self) -> usize {
        self.inner.input_latency()
    }

    pub fn output_latency(&self) -> usize {
        self.inner.output_latency()
    }

    /// `preroll` is the audio just before the first input that `process` will receive.
    pub fn prime(&mut self, preroll: &[f32], speed: f64) {
        self.inner.reset();
        self.inner.seek(preroll, speed);
    }

    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        self.inner.process(input, output);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrexVector {
    pub scope: f64,
    pub abstraction: f64,
    pub context_span: f64,
    pub execution_proximity: f64,
    pub blast_radius: f64,
    pub coordination_load: f64,
    pub latency_pressure: f64,
}

impl DrexVector {
    pub fn axes(&self) -> [f64; 7] {
        [
            self.scope,
            self.abstraction,
            self.context_span,
            self.execution_proximity,
            self.blast_radius,
            self.coordination_load,
            self.latency_pressure,
        ]
    }
}

use std::fmt;

/// A small interface for a system of ordinary differential equations.
pub trait DynamicalSystem {
    fn derivative(&self, time: f64, state: &[f64], output: &mut [f64]);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationError {
    InvalidStep,
    NonFiniteTime,
    NonFiniteState,
    NonFiniteDerivative,
    TimeOverflow,
    StepCounterOverflow,
}

impl fmt::Display for SimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStep => write!(formatter, "simulation step must be finite and positive"),
            Self::NonFiniteTime => write!(formatter, "simulation time must be finite"),
            Self::NonFiniteState => write!(formatter, "simulation state contains NaN or infinity"),
            Self::NonFiniteDerivative => {
                write!(formatter, "dynamical system derivative produced NaN or infinity")
            }
            Self::TimeOverflow => write!(formatter, "simulation time overflowed the finite range"),
            Self::StepCounterOverflow => write!(formatter, "simulation step counter overflowed"),
        }
    }
}

impl std::error::Error for SimulationError {}

/// Reusable scratch storage for explicit integration methods.
///
/// Keeping this object across simulation steps removes the five hot-loop allocations that the
/// original RK4 helper performed on every invocation.
#[derive(Clone, Debug, Default)]
pub struct IntegrationWorkspace {
    k1: Vec<f64>,
    k2: Vec<f64>,
    k3: Vec<f64>,
    k4: Vec<f64>,
    scratch: Vec<f64>,
}

impl IntegrationWorkspace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(state_len: usize) -> Self {
        let mut workspace = Self::default();
        workspace.resize(state_len);
        workspace
    }

    fn resize(&mut self, len: usize) {
        self.k1.resize(len, 0.0);
        self.k2.resize(len, 0.0);
        self.k3.resize(len, 0.0);
        self.k4.resize(len, 0.0);
        self.scratch.resize(len, 0.0);
    }

    pub fn capacity(&self) -> usize {
        self.scratch.capacity()
    }
}

fn validate_inputs(time: f64, step: f64, state: &[f64]) -> Result<(), SimulationError> {
    if !time.is_finite() {
        return Err(SimulationError::NonFiniteTime);
    }
    if !step.is_finite() || step <= 0.0 {
        return Err(SimulationError::InvalidStep);
    }
    if state.iter().any(|value| !value.is_finite()) {
        return Err(SimulationError::NonFiniteState);
    }
    Ok(())
}

fn validate_derivative(values: &[f64]) -> Result<(), SimulationError> {
    if values.iter().any(|value| !value.is_finite()) {
        Err(SimulationError::NonFiniteDerivative)
    } else {
        Ok(())
    }
}

/// Performs one explicit Euler step using caller-owned reusable storage.
pub fn euler_step_with_workspace<S: DynamicalSystem>(
    system: &S,
    time: f64,
    step: f64,
    state: &mut [f64],
    workspace: &mut IntegrationWorkspace,
) -> Result<(), SimulationError> {
    validate_inputs(time, step, state)?;
    workspace.resize(state.len());
    workspace.k1.fill(0.0);
    system.derivative(time, state, &mut workspace.k1);
    validate_derivative(&workspace.k1)?;

    for (value, slope) in state.iter_mut().zip(&workspace.k1) {
        let next = slope.mul_add(step, *value);
        if !next.is_finite() {
            return Err(SimulationError::NonFiniteState);
        }
        *value = next;
    }
    Ok(())
}

/// Convenience Euler step. Repeated simulations should prefer [`euler_step_with_workspace`].
pub fn euler_step<S: DynamicalSystem>(
    system: &S,
    time: f64,
    step: f64,
    state: &mut [f64],
) -> Result<(), SimulationError> {
    let mut workspace = IntegrationWorkspace::with_capacity(state.len());
    euler_step_with_workspace(system, time, step, state, &mut workspace)
}

/// Performs one classical fourth-order Runge-Kutta step using reusable storage.
pub fn rk4_step_with_workspace<S: DynamicalSystem>(
    system: &S,
    time: f64,
    step: f64,
    state: &mut [f64],
    workspace: &mut IntegrationWorkspace,
) -> Result<(), SimulationError> {
    validate_inputs(time, step, state)?;
    let half_time = time + step * 0.5;
    let end_time = time + step;
    if !half_time.is_finite() || !end_time.is_finite() {
        return Err(SimulationError::TimeOverflow);
    }

    workspace.resize(state.len());
    workspace.k1.fill(0.0);
    system.derivative(time, state, &mut workspace.k1);
    validate_derivative(&workspace.k1)?;

    for index in 0..state.len() {
        workspace.scratch[index] = state[index] + workspace.k1[index] * step * 0.5;
    }
    validate_derivative(&workspace.scratch).map_err(|_| SimulationError::NonFiniteState)?;

    workspace.k2.fill(0.0);
    system.derivative(half_time, &workspace.scratch, &mut workspace.k2);
    validate_derivative(&workspace.k2)?;

    for index in 0..state.len() {
        workspace.scratch[index] = state[index] + workspace.k2[index] * step * 0.5;
    }
    validate_derivative(&workspace.scratch).map_err(|_| SimulationError::NonFiniteState)?;

    workspace.k3.fill(0.0);
    system.derivative(half_time, &workspace.scratch, &mut workspace.k3);
    validate_derivative(&workspace.k3)?;

    for index in 0..state.len() {
        workspace.scratch[index] = state[index] + workspace.k3[index] * step;
    }
    validate_derivative(&workspace.scratch).map_err(|_| SimulationError::NonFiniteState)?;

    workspace.k4.fill(0.0);
    system.derivative(end_time, &workspace.scratch, &mut workspace.k4);
    validate_derivative(&workspace.k4)?;

    // Compute into scratch first so a non-finite component cannot leave a partially committed state.
    for index in 0..state.len() {
        let weighted = workspace.k1[index]
            + 2.0 * workspace.k2[index]
            + 2.0 * workspace.k3[index]
            + workspace.k4[index];
        let next = state[index] + step * weighted / 6.0;
        if !next.is_finite() {
            return Err(SimulationError::NonFiniteState);
        }
        workspace.scratch[index] = next;
    }
    state.copy_from_slice(&workspace.scratch);
    Ok(())
}

/// Convenience RK4 step. Hot loops should retain an [`IntegrationWorkspace`].
pub fn rk4_step<S: DynamicalSystem>(
    system: &S,
    time: f64,
    step: f64,
    state: &mut [f64],
) -> Result<(), SimulationError> {
    let mut workspace = IntegrationWorkspace::with_capacity(state.len());
    rk4_step_with_workspace(system, time, step, state, &mut workspace)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimulationClock {
    pub time: f64,
    pub step_index: u64,
}

impl SimulationClock {
    pub fn advance(&mut self, step: f64) -> Result<(), SimulationError> {
        if !self.time.is_finite() {
            return Err(SimulationError::NonFiniteTime);
        }
        if !step.is_finite() || step <= 0.0 {
            return Err(SimulationError::InvalidStep);
        }
        let next_time = self.time + step;
        if !next_time.is_finite() {
            return Err(SimulationError::TimeOverflow);
        }
        let next_step = self
            .step_index
            .checked_add(1)
            .ok_or(SimulationError::StepCounterOverflow)?;
        self.time = next_time;
        self.step_index = next_step;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DynamicalSystem, IntegrationWorkspace, SimulationClock, SimulationError, euler_step,
        rk4_step_with_workspace,
    };

    struct ConstantAcceleration;

    impl DynamicalSystem for ConstantAcceleration {
        fn derivative(&self, _time: f64, _state: &[f64], output: &mut [f64]) {
            output.fill(2.0);
        }
    }

    struct BrokenDerivative;

    impl DynamicalSystem for BrokenDerivative {
        fn derivative(&self, _time: f64, _state: &[f64], output: &mut [f64]) {
            output.fill(f64::NAN);
        }
    }

    #[test]
    fn euler_step_updates_state() {
        let mut state = [1.0, -1.0];
        euler_step(&ConstantAcceleration, 0.0, 0.5, &mut state).expect("valid step");
        assert_eq!(state, [2.0, 0.0]);
    }

    #[test]
    fn integration_rejects_non_finite_inputs_and_derivatives() {
        let mut state = [0.0];
        assert_eq!(
            euler_step(&ConstantAcceleration, 0.0, f64::NAN, &mut state),
            Err(SimulationError::InvalidStep)
        );
        assert_eq!(
            euler_step(&BrokenDerivative, 0.0, 0.1, &mut state),
            Err(SimulationError::NonFiniteDerivative)
        );
    }

    #[test]
    fn rk4_workspace_is_reused_across_steps() {
        let mut state = [1.0];
        let mut workspace = IntegrationWorkspace::with_capacity(state.len());
        let initial_capacity = workspace.capacity();
        for step_index in 0..10 {
            rk4_step_with_workspace(
                &ConstantAcceleration,
                f64::from(step_index) * 0.5,
                0.5,
                &mut state,
                &mut workspace,
            )
            .expect("valid step");
        }
        assert!((state[0] - 11.0).abs() < 0.000_001);
        assert!(workspace.capacity() >= initial_capacity);
    }

    #[test]
    fn clock_rejects_non_finite_step_without_mutating_state() {
        let mut clock = SimulationClock::default();
        assert_eq!(clock.advance(f64::INFINITY), Err(SimulationError::InvalidStep));
        assert_eq!(clock, SimulationClock::default());
    }
}
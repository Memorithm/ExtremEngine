use extrem_ecs::{World, WorldError};

/// Engine execution phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    Startup,
    FixedUpdate,
    Update,
    PostUpdate,
    Render,
}

/// Time information passed to every system for a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Time {
    pub delta_seconds: f32,
    pub elapsed_seconds: f32,
    pub frame: u64,
    pub fixed_delta_seconds: f32,
    pub fixed_elapsed_seconds: f32,
    pub fixed_step: u64,
    pub fixed_steps_this_frame: u32,
}

impl Default for Time {
    fn default() -> Self {
        Self {
            delta_seconds: 0.0,
            elapsed_seconds: 0.0,
            frame: 0,
            fixed_delta_seconds: 1.0 / 60.0,
            fixed_elapsed_seconds: 0.0,
            fixed_step: 0,
            fixed_steps_this_frame: 0,
        }
    }
}

impl Time {
    fn advance_frame(&mut self, delta_seconds: f32, fixed_delta_seconds: f32) {
        self.delta_seconds = delta_seconds.max(0.0);
        self.elapsed_seconds += self.delta_seconds;
        self.frame = self.frame.saturating_add(1);
        self.fixed_delta_seconds = fixed_delta_seconds;
        self.fixed_steps_this_frame = 0;
    }

    fn advance_fixed_step(&mut self) {
        self.fixed_elapsed_seconds += self.fixed_delta_seconds;
        self.fixed_step = self.fixed_step.saturating_add(1);
        self.fixed_steps_this_frame = self.fixed_steps_this_frame.saturating_add(1);
    }
}

/// Small summary returned after an application update.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UpdateReport {
    pub frame: u64,
    pub delta_seconds: f32,
    pub elapsed_seconds: f32,
}

/// Extension point for engine subsystems.
pub trait Plugin {
    fn build(&self, app: &mut App);

    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Default plugin group for a headless or test-oriented application.
#[derive(Clone, Copy, Debug, Default)]
pub struct MinimalPlugins;

impl Plugin for MinimalPlugins {
    fn build(&self, app: &mut App) {
        app.world.insert_resource(Time::default());
    }
}

type System = Box<dyn FnMut(&mut World, Time)>;

/// Owns the ECS world and executes systems in deterministic stages.
pub struct App {
    pub world: World,
    time: Time,
    fixed_delta_seconds: f32,
    fixed_accumulator: f32,
    max_fixed_steps_per_frame: u32,
    startup_systems: Vec<System>,
    fixed_update_systems: Vec<System>,
    update_systems: Vec<System>,
    post_update_systems: Vec<System>,
    render_systems: Vec<System>,
    startup_complete: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            time: Time::default(),
            fixed_delta_seconds: 1.0 / 60.0,
            fixed_accumulator: 0.0,
            max_fixed_steps_per_frame: 8,
            startup_systems: Vec::new(),
            fixed_update_systems: Vec::new(),
            update_systems: Vec::new(),
            post_update_systems: Vec::new(),
            render_systems: Vec::new(),
            startup_complete: false,
        }
    }

    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        plugin.build(self);
        self
    }

    pub fn add_systems<F>(&mut self, stage: Stage, system: F) -> &mut Self
    where
        F: FnMut(&mut World, Time) + 'static,
    {
        let systems = match stage {
            Stage::Startup => &mut self.startup_systems,
            Stage::FixedUpdate => &mut self.fixed_update_systems,
            Stage::Update => &mut self.update_systems,
            Stage::PostUpdate => &mut self.post_update_systems,
            Stage::Render => &mut self.render_systems,
        };
        systems.push(Box::new(system));
        self
    }

    pub fn set_fixed_timestep(&mut self, fixed_delta_seconds: f32) -> &mut Self {
        if fixed_delta_seconds.is_finite() && fixed_delta_seconds > 0.0 {
            self.fixed_delta_seconds = fixed_delta_seconds;
            self.time.fixed_delta_seconds = fixed_delta_seconds;
        }
        self
    }

    pub fn fixed_timestep(&self) -> f32 {
        self.fixed_delta_seconds
    }

    pub fn set_max_fixed_steps_per_frame(&mut self, max_steps: u32) -> &mut Self {
        self.max_fixed_steps_per_frame = max_steps.max(1);
        self
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn time(&self) -> Time {
        self.time
    }

    pub fn update(&mut self, delta_seconds: f32) -> UpdateReport {
        self.time
            .advance_frame(delta_seconds, self.fixed_delta_seconds);
        self.fixed_accumulator += self.time.delta_seconds;
        let time = self.time;
        self.world.insert_resource(time);

        if !self.startup_complete {
            run_systems(&mut self.startup_systems, &mut self.world, time);
            self.startup_complete = true;
        }
        let mut fixed_steps = 0;
        while self.fixed_accumulator >= self.fixed_delta_seconds
            && fixed_steps < self.max_fixed_steps_per_frame
        {
            self.fixed_accumulator -= self.fixed_delta_seconds;
            self.time.advance_fixed_step();
            let fixed_time = self.time;
            self.world.insert_resource(fixed_time);
            run_systems(&mut self.fixed_update_systems, &mut self.world, fixed_time);
            fixed_steps += 1;
        }
        let frame_time = self.time;
        self.world.insert_resource(frame_time);
        run_systems(&mut self.update_systems, &mut self.world, frame_time);
        run_systems(&mut self.post_update_systems, &mut self.world, frame_time);
        run_systems(&mut self.render_systems, &mut self.world, frame_time);

        UpdateReport {
            frame: frame_time.frame,
            delta_seconds: frame_time.delta_seconds,
            elapsed_seconds: frame_time.elapsed_seconds,
        }
    }

    pub fn run_for(&mut self, frames: usize, delta_seconds: f32) -> Vec<UpdateReport> {
        (0..frames).map(|_| self.update(delta_seconds)).collect()
    }
}

fn run_systems(systems: &mut [System], world: &mut World, time: Time) {
    for system in systems {
        system(world, time);
    }
}

/// Convenience result type for systems that perform world operations.
pub type AppResult<T> = Result<T, WorldError>;

#[cfg(test)]
mod tests {
    use super::{App, MinimalPlugins, Stage};

    #[test]
    fn startup_runs_once_and_update_runs_every_frame() {
        let mut app = App::new();
        app.add_plugin(MinimalPlugins);
        app.add_systems(Stage::Startup, |world, _| {
            world.insert_resource(1_u32);
        });
        app.add_systems(Stage::Update, |world, _| {
            let value = world.get_resource_mut::<u32>().expect("startup resource");
            *value += 1;
        });

        app.run_for(3, 1.0 / 60.0);
        assert_eq!(app.world().get_resource::<u32>(), Some(&4));
        assert_eq!(app.time().frame, 3);
    }

    #[test]
    fn fixed_update_is_independent_from_render_delta() {
        let mut app = App::new();
        app.set_fixed_timestep(0.1);
        app.add_systems(Stage::FixedUpdate, |world, _| {
            let value = world.get_resource_mut::<u32>().expect("counter");
            *value += 1;
        });
        app.world_mut().insert_resource(0_u32);

        app.update(0.25);
        assert_eq!(app.world().get_resource::<u32>(), Some(&2));
        assert_eq!(app.time().fixed_steps_this_frame, 2);
        assert_eq!(app.time().fixed_step, 2);
    }
}

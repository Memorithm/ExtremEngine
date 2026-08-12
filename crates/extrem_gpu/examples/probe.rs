use extrem_gpu::GpuContext;

fn main() {
    match GpuContext::headless() {
        Ok(context) => println!("GPU adapter: {}", context.adapter_name()),
        Err(error) => println!("GPU unavailable: {error}"),
    }
}

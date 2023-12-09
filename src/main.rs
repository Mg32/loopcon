use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use std::sync::atomic::{self, AtomicBool};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let latency = 60.0;

    // find devices
    let host = cpal::default_host();
    let device_in = host
        .default_input_device()
        .expect("Failed to find input device!");
    let device_out = host
        .default_output_device()
        .expect("Failed to find output device!");
    println!("Device I: [{}]", device_in.name()?);
    println!("Device O: [{}]", device_out.name()?);

    // calculate the latency
    let config: cpal::StreamConfig = device_in.default_input_config()?.into();
    let latency_frames = (latency / 1000.0) * config.sample_rate.0 as f32;
    let latency_samples = latency_frames as usize * config.channels as usize;

    // create a ring buffer
    let ring = HeapRb::<f32>::new(latency_samples * 2);
    let (mut producer, mut consumer) = ring.split();

    // fill the samples with 0.0 equal to the length of the delay
    for _ in 0..latency_samples {
        producer.push(0.0).unwrap();
    }

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;
        for &sample in data {
            if producer.push(sample).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("output stream fell behind. try increasing latency!");
        }
    };

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut input_fell_behind = false;
        for sample in data {
            *sample = match consumer.pop() {
                Some(s) => s,
                None => {
                    input_fell_behind = true;
                    0.0
                }
            };
        }
        if input_fell_behind {
            eprintln!("input stream fell behind. try increasing latency!");
        }
    };

    let err_fn = move |err: cpal::StreamError| {
        eprintln!("an error occurred on stream: {}", err);
    };

    // build streams
    println!("config: {:?}", config);
    let stream_in = device_in.build_input_stream(&config, input_data_fn, err_fn, None)?;
    let stream_out = device_out.build_output_stream(&config, output_data_fn, err_fn, None)?;

    // play the streams
    stream_in.play()?;
    stream_out.play()?;

    println!("press Ctrl-C to exit...");
    wait_for_break();

    println!("shutdown...");
    drop(stream_in);
    drop(stream_out);
    Ok(())
}

fn wait_for_break() {
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Ctrl-C handler
    ctrlc::set_handler(move || {
        running_clone.store(false, atomic::Ordering::Relaxed);
    })
    .expect("error setting Ctrl-C handler");

    // wait until the `running` flag has set
    while running.load(atomic::Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(100));
    }
}

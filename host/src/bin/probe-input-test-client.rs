// MoonBeam M2 step 3 — scripted input client.
//
// Connects to probe-input-server's /ws and fires a fixed sequence of
// input events (pen tap, pen stroke, two-finger pinch, single-finger
// tap), then exits. Used to validate the full WS-to-uinput path end
// to end without a browser in the loop.
//
// Wire format matches probe-input-server: [0x03][flags=0][json].
//
// Run with the server already up on :7879:
//   cargo run --bin probe-input-test-client

use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use futures_util::SinkExt;
use serde_json::json;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message;

const TYPE_INPUT: u8 = 0x03;
const FLAG_NONE: u8 = 0x00;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "ws://127.0.0.1:7879/ws")]
    url: String,
    /// Coordinate space (must match server's --width/--height).
    #[arg(long, default_value_t = 2960)]
    width: i32,
    #[arg(long, default_value_t = 1848)]
    height: i32,
    #[arg(long, default_value_t = 4095)]
    pressure_max: i32,
    /// Pen-stroke samples (60 ≈ smooth at 1.5s default duration).
    #[arg(long, default_value_t = 60)]
    samples: u32,
    #[arg(long, default_value_t = 1500)]
    stroke_ms: u64,
    /// Skip the touch portion (only fire pen events).
    #[arg(long)]
    pen_only: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("=== MoonBeam M2 step 3 — scripted input test client ===");
    println!("connecting to {}", cli.url);
    let (mut ws, _) = tokio_tungstenite::connect_async(&cli.url)
        .await
        .with_context(|| format!("connect {}", cli.url))?;
    println!("connected");

    let cx = cli.width / 2;
    let cy = cli.height / 2;

    // 1. Pen tap at center.
    println!("→ pen tap at ({cx}, {cy})");
    send(
        &mut ws,
        json!({"type":"pen_down","x":cx,"y":cy,"pressure":cli.pressure_max/2,"tilt_x":0,"tilt_y":0}),
    )
    .await?;
    sleep(Duration::from_millis(50)).await;
    send(&mut ws, json!({"type":"pen_up"})).await?;
    sleep(Duration::from_millis(200)).await;

    // 2. Pen stroke: left-edge to right-edge with triangular pressure.
    println!(
        "→ pen stroke 0→{} at y={}, {} samples over {} ms",
        cli.width - 1,
        cy,
        cli.samples,
        cli.stroke_ms
    );
    send(
        &mut ws,
        json!({"type":"pen_down","x":0,"y":cy,"pressure":1,"tilt_x":0,"tilt_y":0}),
    )
    .await?;
    let dt_ms = cli.stroke_ms / cli.samples as u64;
    for i in 1..=cli.samples {
        let t = i as f32 / cli.samples as f32;
        let x = (t * (cli.width - 1) as f32) as i32;
        let pressure = if t <= 0.5 {
            (t / 0.5 * cli.pressure_max as f32) as i32
        } else {
            ((1.0 - t) / 0.5 * cli.pressure_max as f32) as i32
        };
        send(
            &mut ws,
            json!({"type":"pen_move","x":x,"y":cy,"pressure":pressure.max(1),"tilt_x":0,"tilt_y":0}),
        )
        .await?;
        sleep(Duration::from_millis(dt_ms)).await;
    }
    send(&mut ws, json!({"type":"pen_up"})).await?;
    sleep(Duration::from_millis(200)).await;

    // 3. Stylus button press + release.
    println!("→ stylus button toggle");
    send(
        &mut ws,
        json!({"type":"pen_button","button":"stylus","state":true}),
    )
    .await?;
    sleep(Duration::from_millis(150)).await;
    send(
        &mut ws,
        json!({"type":"pen_button","button":"stylus","state":false}),
    )
    .await?;
    sleep(Duration::from_millis(200)).await;

    if !cli.pen_only {
        // 4. Two-finger pinch-out.
        let left = cli.width / 4;
        let right = cli.width - left;
        println!("→ two-finger pinch-out from center to ({left},{cy}) / ({right},{cy})");
        send(
            &mut ws,
            json!({"type":"touch_down","slot":0,"id":1001,"x":cx,"y":cy,"major":200,"pressure":100}),
        )
        .await?;
        send(
            &mut ws,
            json!({"type":"touch_down","slot":1,"id":1002,"x":cx,"y":cy,"major":200,"pressure":100}),
        )
        .await?;
        for i in 1..=cli.samples {
            let t = i as f32 / cli.samples as f32;
            let x0 = cx + ((left - cx) as f32 * t) as i32;
            let x1 = cx + ((right - cx) as f32 * t) as i32;
            send(
                &mut ws,
                json!({"type":"touch_move","slot":0,"x":x0,"y":cy,"major":200,"pressure":100}),
            )
            .await?;
            send(
                &mut ws,
                json!({"type":"touch_move","slot":1,"x":x1,"y":cy,"major":200,"pressure":100}),
            )
            .await?;
            sleep(Duration::from_millis(dt_ms)).await;
        }
        send(&mut ws, json!({"type":"touch_up","slot":0})).await?;
        send(&mut ws, json!({"type":"touch_up","slot":1})).await?;
        sleep(Duration::from_millis(200)).await;

        // 5. Single-finger tap.
        println!("→ single-finger tap at ({cx},{cy})");
        send(
            &mut ws,
            json!({"type":"touch_down","slot":0,"id":2001,"x":cx,"y":cy,"major":200,"pressure":100}),
        )
        .await?;
        sleep(Duration::from_millis(50)).await;
        send(&mut ws, json!({"type":"touch_up","slot":0})).await?;
    }

    sleep(Duration::from_millis(200)).await;
    println!("done");
    ws.send(Message::Close(None)).await.ok();
    Ok(())
}

async fn send<S>(ws: &mut S, value: serde_json::Value) -> Result<()>
where
    S: futures_util::Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let json = serde_json::to_vec(&value).context("serialize JSON")?;
    let mut frame = Vec::with_capacity(json.len() + 2);
    frame.push(TYPE_INPUT);
    frame.push(FLAG_NONE);
    frame.extend_from_slice(&json);
    ws.send(Message::Binary(frame.into()))
        .await
        .context("ws send")?;
    Ok(())
}

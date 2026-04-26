// MoonBeam M1 step 1 — DRM writeback probe.
//
// Confirms that vkms exposes a Writeback connector at the kernel level and
// dumps its properties (especially WRITEBACK_PIXEL_FORMATS) so we know what
// surface formats to request when we move on to actually pulling frames.
//
// Run with `cargo run --bin probe-writeback -- /dev/dri/card0` (vkms must
// be loaded with enable_writeback=1).

use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, BorrowedFd};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use drm::control::{connector, Device as ControlDevice};
use drm::{ClientCapability, Device as BasicDevice};

#[derive(Parser)]
#[command(about = "Probe a DRM device for the writeback connector and dump its props")]
struct Cli {
    /// Path to the DRM card device (vkms is usually /dev/dri/card0)
    #[arg(default_value = "/dev/dri/card0")]
    device: PathBuf,
}

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl BasicDevice for Card {}
impl ControlDevice for Card {}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&cli.device)
        .with_context(|| format!("opening {}", cli.device.display()))?;
    let card = Card(file);

    println!("=== device: {} ===", cli.device.display());

    // Atomic must be enabled before writeback, per drm_ioctl.c:
    // DRM_CLIENT_CAP_WRITEBACK_CONNECTORS rejects with -EINVAL otherwise.
    card.set_client_capability(ClientCapability::Atomic, true)
        .context("DRM_CLIENT_CAP_ATOMIC")?;
    card.set_client_capability(ClientCapability::WritebackConnectors, true)
        .context("DRM_CLIENT_CAP_WRITEBACK_CONNECTORS")?;

    let res = card.resource_handles().context("resource_handles")?;
    println!("connectors found: {}", res.connectors().len());

    let mut writeback: Option<connector::Handle> = None;

    for &handle in res.connectors() {
        let info = card
            .get_connector(handle, false)
            .with_context(|| format!("get_connector {handle:?}"))?;

        let kind = info.interface();
        let state = info.state();
        println!(
            "  connector handle={:?} kind={:?} state={:?} modes={}",
            handle,
            kind,
            state,
            info.modes().len()
        );

        if matches!(kind, connector::Interface::Writeback) {
            writeback = Some(handle);
        }
    }

    let Some(wb) = writeback else {
        anyhow::bail!(
            "no Writeback connector on {}. Is vkms loaded with enable_writeback=1?",
            cli.device.display()
        );
    };

    println!("\n=== writeback connector handle={wb:?} properties ===");

    let props = card
        .get_properties(wb)
        .context("get_properties on writeback connector")?;

    let mut writeback_formats_blob: Option<u64> = None;
    for (prop_id, value) in props.iter() {
        let info = card
            .get_property(*prop_id)
            .with_context(|| format!("get_property {prop_id:?}"))?;
        let name = info.name().to_string_lossy();
        println!("  {name:<32} id={:?}  raw_value={value}", prop_id);
        if name == "WRITEBACK_PIXEL_FORMATS" {
            writeback_formats_blob = Some(*value);
        }
    }

    if let Some(blob_id) = writeback_formats_blob {
        let bytes = card
            .get_property_blob(blob_id)
            .context("get_property_blob WRITEBACK_PIXEL_FORMATS")?;
        println!(
            "\n=== WRITEBACK_PIXEL_FORMATS blob ({} bytes, {} fourccs) ===",
            bytes.len(),
            bytes.len() / 4
        );
        for chunk in bytes.chunks_exact(4) {
            let fourcc = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let s: String = chunk
                .iter()
                .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
                .collect();
            println!("  0x{fourcc:08x}  '{s}'");
        }
    }

    println!("\nOK — writeback connector enumerated successfully.");
    Ok(())
}

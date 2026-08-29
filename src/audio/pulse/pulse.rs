use crate::audio::{AppInfo, AudioSystem};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use pulsectl::controllers::{AppControl, DeviceControl, SinkController};
use std::error::Error;

const PA_VOLUME_NORM: u32 = 98304; // 150% in PulseAudio

pub struct PulseAudioSystem {
    controller: SinkController,
}

impl PulseAudioSystem {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            controller: SinkController::create()?,
        })
    }
}

/// One raw PulseAudio sink-input, before streams belonging to the same app
/// instance are folded together into a single `AppInfo`.
struct RawStream {
    uid: u32,
    app_name: String,
    /// `application.process.id`, used to group multiple streams opened by the
    /// same running process (e.g. a game with separate music/SFX buses).
    pid: Option<u32>,
    mute: bool,
    vol_percent: f32,
    icon_name: Option<String>,
}

impl AudioSystem for PulseAudioSystem {
    fn list_applications(&mut self) -> Result<Vec<AppInfo>, Box<dyn Error>> {
        let mut res: Vec<AppInfo> = Vec::new();

        // Add the default system sink (main PC audio) only if the global flag is set.
        // It's never grouped with anything else.
        if crate::utils::should_show_system_mixer()
            && let Ok(default_sink) = self.controller.get_default_device()
        {
            let system_name = default_sink
                .description
                .clone()
                .unwrap_or("System Audio".to_string());

            res.push(AppInfo {
                member_uids: vec![default_sink.index],
                app_name: system_name,
                mute: default_sink.mute,
                vol_percent: get_pulse_app_volume_percentage(&default_sink.volume),
                icon_name: Some("audio-card".to_string()),
                is_device: true,
            });
        }

        let apps = self.controller.list_applications()?;

        let raw_streams: Vec<RawStream> = apps
            .into_iter()
            .map(|app| RawStream {
                uid: app.index,
                app_name: app
                    .proplist
                    .get_str("application.name")
                    .unwrap_or("app_stream".to_string())
                    .to_lowercase(),
                pid: app
                    .proplist
                    .get_str("application.process.id")
                    .and_then(|pid| pid.parse().ok()),
                mute: app.mute,
                vol_percent: get_pulse_app_volume_percentage(&app.volume),
                icon_name: app.proplist.get_str("application.icon_name"),
            })
            .collect();

        res.extend(group_streams(raw_streams));

        Ok(res)
    }

    fn set_volume(
        &mut self,
        app_index: u32,
        percent: f32,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>> {
        let target = Volume(((percent.clamp(0.0, 100.0) / 100.0) * PA_VOLUME_NORM as f32) as u32);

        if is_device {
            let device = self.controller.get_device_by_index(app_index)?;
            let mut volumes = device.volume;
            volumes.set(volumes.len(), target);
            self.controller.set_device_volume_by_index(app_index, &volumes);
        } else {
            let app = self.controller.get_app_by_index(app_index)?;
            let mut volumes = app.volume;
            volumes.set(volumes.len(), target);
            let op = self
                .controller
                .handler
                .introspect
                .set_sink_input_volume(app_index, &volumes, None);
            self.controller.handler.wait_for_operation(op)?;
        }

        Ok(())
    }

    fn mute_volume(
        &mut self,
        app_index: u32,
        mute: bool,
        is_device: bool,
    ) -> Result<(), Box<dyn Error>> {
        if is_device {
            self.controller.set_device_mute_by_index(app_index, mute);
        } else {
            self.controller.set_app_mute(app_index, mute)?;
        }
        Ok(())
    }
}

/// Fold raw sink-inputs that belong to the same app instance (matching
/// app name + PID) into a single `AppInfo`, preserving first-seen order.
fn group_streams(raw_streams: Vec<RawStream>) -> Vec<AppInfo> {
    // (grouping key, accumulated volume sum, member count) kept alongside
    // `groups` (same index) since `AppInfo` itself has no PID field.
    let mut keys: Vec<(String, Option<u32>)> = Vec::new();
    let mut vol_sums: Vec<f32> = Vec::new();
    let mut vol_counts: Vec<u32> = Vec::new();
    let mut groups: Vec<AppInfo> = Vec::new();

    for stream in raw_streams {
        let key = (stream.app_name.clone(), stream.pid);
        let existing = keys.iter().position(|k| *k == key);

        match existing {
            Some(idx) => {
                let group = &mut groups[idx];
                group.member_uids.push(stream.uid);
                group.mute = group.mute && stream.mute;
                if group.icon_name.is_none() {
                    group.icon_name = stream.icon_name;
                }
                vol_sums[idx] += stream.vol_percent;
                vol_counts[idx] += 1;
            }
            None => {
                keys.push(key);
                vol_sums.push(stream.vol_percent);
                vol_counts.push(1);
                groups.push(AppInfo {
                    member_uids: vec![stream.uid],
                    app_name: stream.app_name,
                    mute: stream.mute,
                    vol_percent: stream.vol_percent,
                    icon_name: stream.icon_name,
                    is_device: false,
                });
            }
        }
    }

    for (i, group) in groups.iter_mut().enumerate() {
        group.vol_percent = vol_sums[i] / vol_counts[i] as f32;
        group.member_uids.sort_unstable();
    }

    groups
}

fn get_pulse_app_volume_percentage(channel_volumes: &ChannelVolumes) -> f32 {
    let channel_count = channel_volumes.len();
    if channel_count == 0 {
        return 0.0;
    }

    // Get average of all channels
    let total_volume: u32 = (0..channel_count)
        .map(|i| channel_volumes.get()[i as usize].0)
        .sum();

    let avg_volume = total_volume as f32 / channel_count as f32;
    let perc = (avg_volume / PA_VOLUME_NORM as f32) * 100.0;

    perc.min(100.0)
}

use crate::audio::{AppInfo, AudioSystem};
use libpulse_binding::volume::{ChannelVolumes, Volume};
use pulsectl::controllers::{AppControl, DeviceControl, SinkController};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{LazyLock, Mutex};

const PA_VOLUME_NORM: u32 = 98304; // 150% in PulseAudio

/// Identifies a single PulseAudio volume target: a device or an app
/// sink-input, by its index. Devices and sink-inputs are separate index
/// spaces, so the index alone isn't a unique key.
type StreamKey = (bool, u32);

/// Per-stream channel balance, as each channel's fixed deviation (in raw
/// PulseAudio volume units) from the group's average — e.g. `[-10, +10]`
/// for a 20-point gap between two channels. Used to re-derive full-precision
/// channel volumes from a target average even after a channel has been
/// clamped to 0% or 100%, so the balance is restored exactly once the
/// average moves back into range. See `apply_target_volume`.
///
/// This has to be a process-wide static rather than a field on
/// `PulseAudioSystem`: callers construct a fresh `PulseAudioSystem` for
/// every single volume adjustment (see `audio::create()`), so anything
/// stored on `self` never survives from one dial tick to the next.
static CHANNEL_OFFSETS: LazyLock<Mutex<HashMap<StreamKey, Vec<f64>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub struct PulseAudioSystem {
    controller: SinkController,
}

impl PulseAudioSystem {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            controller: SinkController::create()?,
        })
    }

    /// Sets `volumes` so the average of its channels becomes `target`, while
    /// keeping a fixed point-gap between channels (e.g. a channel that's 20
    /// points quieter than another stays exactly 20 points quieter, however
    /// the average moves) and never letting any single channel go below 0%
    /// or above 100%.
    ///
    /// The per-channel offset from the average is cached in `CHANNEL_OFFSETS`
    /// rather than re-derived from `volumes` on every call, because once a
    /// channel is clamped at a boundary PulseAudio only remembers the
    /// clamped value — the original gap is gone from the live data.
    /// Re-deriving from live values in that state would permanently corrupt
    /// the gap on the very next call. Instead, whenever the live channels
    /// are NOT at a boundary, they're trusted and cached; whenever they are,
    /// the cached offsets (from before the boundary was hit) are used to
    /// recompute the channels analytically, so turning the dial back the
    /// other way restores the exact original gap as soon as it's
    /// representable again.
    fn apply_target_volume(&mut self, key: StreamKey, volumes: &mut ChannelVolumes, target: Volume) {
        let channel_count = volumes.len();
        if channel_count == 0 {
            return;
        }
        let channel_count = channel_count as usize;

        let live: Vec<u32> = volumes.get().iter().map(|v| v.0).collect();
        let live_at_boundary = live.iter().any(|&v| v == 0 || v >= PA_VOLUME_NORM);

        let mut cache = CHANNEL_OFFSETS.lock().unwrap();
        let cached = cache.get(&key).filter(|o| o.len() == channel_count);

        let offsets: Vec<f64> = if !live_at_boundary {
            let live_avg = live.iter().map(|&v| v as f64).sum::<f64>() / channel_count as f64;
            live.iter().map(|&v| v as f64 - live_avg).collect()
        } else if let Some(o) = cached {
            o.clone()
        } else {
            let live_avg = live.iter().map(|&v| v as f64).sum::<f64>() / channel_count as f64;
            live.iter().map(|&v| v as f64 - live_avg).collect()
        };

        cache.insert(key, offsets.clone());
        drop(cache);

        let targets = distribute_with_fixed_offsets(&offsets, target.0 as f64, PA_VOLUME_NORM as f64);
        for (v, t) in volumes.get_mut().iter_mut().zip(targets.iter()) {
            v.0 = t.round().clamp(0.0, PA_VOLUME_NORM as f64) as u32;
        }
    }
}

/// Distributes `target_avg` across channels so their mean equals
/// `target_avg` and each channel `i`'s value stays exactly `offsets[i]`
/// points away from that shared base, for as long as that's physically
/// possible within `[0, ceiling]`.
///
/// A plain "shift every channel by the same amount, then clamp" does *not*
/// do this: once a channel clamps, the excess it couldn't take is simply
/// lost instead of handed to the other channels, so the quietest channel
/// can never reach `ceiling` even at `target_avg == ceiling`, and coming
/// back down immediately resumes the fixed gap instead of first releasing
/// the clamped channel. Here, a channel that would need to go outside `[0,
/// ceiling]` to keep the exact gap is pinned at that bound instead, and the
/// remaining budget is redistributed among the other channels — still
/// keeping their own gaps relative to each other — so the average stays
/// correct and every channel that *can* still move does. This is the
/// standard "water-filling" allocation (additive form), and it's what makes
/// the result exactly reversible: raising `target_avg` pins channels onto
/// the ceiling one at a time as they reach it, and lowering it releases
/// them in the reverse order, landing back on the original gap once none
/// are pinned anymore.
fn distribute_with_fixed_offsets(offsets: &[f64], target_avg: f64, ceiling: f64) -> Vec<f64> {
    let n = offsets.len();
    let mut pinned: Vec<Option<f64>> = vec![None; n];
    let total_budget = target_avg * n as f64;

    loop {
        let pinned_sum: f64 = pinned.iter().flatten().sum();
        let free: Vec<usize> = (0..n).filter(|&i| pinned[i].is_none()).collect();
        if free.is_empty() {
            break;
        }

        let free_count = free.len() as f64;
        let free_offset_sum: f64 = free.iter().map(|&i| offsets[i]).sum();
        let base = (total_budget - pinned_sum - free_offset_sum) / free_count;

        let mut any_violation = false;
        for &i in &free {
            let v = base + offsets[i];
            if v > ceiling {
                pinned[i] = Some(ceiling);
                any_violation = true;
            } else if v < 0.0 {
                pinned[i] = Some(0.0);
                any_violation = true;
            }
        }

        if !any_violation {
            for &i in &free {
                pinned[i] = Some(base + offsets[i]);
            }
            break;
        }
    }

    pinned.into_iter().map(|p| p.unwrap_or(0.0)).collect()
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
        let mut live_keys: Vec<StreamKey> = Vec::new();

        // Add the default system sink (main PC audio) only if the global flag is set.
        // It's never grouped with anything else.
        if crate::utils::should_show_system_mixer()
            && let Ok(default_sink) = self.controller.get_default_device()
        {
            live_keys.push((true, default_sink.index));
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

        live_keys.extend(raw_streams.iter().map(|s| (false, s.uid)));

        res.extend(group_streams(raw_streams));

        // Drop cached channel balances for streams/devices that are gone, so
        // the map doesn't grow forever as apps open and close.
        CHANNEL_OFFSETS.lock().unwrap().retain(|key, _| live_keys.contains(key));

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
            self.apply_target_volume((true, app_index), &mut volumes, target);
            self.controller.set_device_volume_by_index(app_index, &volumes);
        } else {
            let app = self.controller.get_app_by_index(app_index)?;
            let mut volumes = app.volume;
            self.apply_target_volume((false, app_index), &mut volumes, target);
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

/// Grouping key for a stream: its real PID when known, or its own uid as a
/// fallback so streams without a PID never get merged with one another.
#[derive(PartialEq, Eq, Clone, Copy)]
enum GroupId {
    Pid(u32),
    Uid(u32),
}

/// Fold raw sink-inputs that belong to the same app instance (matching
/// app name + PID) into a single `AppInfo`, preserving first-seen order.
fn group_streams(raw_streams: Vec<RawStream>) -> Vec<AppInfo> {
    // (grouping key, accumulated volume sum, member count) kept alongside
    // `groups` (same index) since `AppInfo` itself has no PID field.
    let mut keys: Vec<(String, GroupId)> = Vec::new();
    let mut vol_sums: Vec<f32> = Vec::new();
    let mut vol_counts: Vec<u32> = Vec::new();
    let mut groups: Vec<AppInfo> = Vec::new();

    for stream in raw_streams {
        let group_id = stream.pid.map(GroupId::Pid).unwrap_or(GroupId::Uid(stream.uid));
        let key = (stream.app_name.clone(), group_id);
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

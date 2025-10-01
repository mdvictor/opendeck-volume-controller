use std::collections::HashMap;

use openaction::*;

use crate::{
    audio::{self, *},
    gfx::{self, TRANSPARENT_ICON},
    utils::{self, update_header},
};

// this could be a plugin setting
const VOLUME_INCREMENT: f32 = 0.1;
const VOLUME_INCREMENT_PERCENTAGE: f32 = VOLUME_INCREMENT * 100.0;

pub struct VCAction;
#[async_trait]
impl Action for VCAction {
    const UUID: ActionUuid = "com.victormarin.volume-controller.auto-detection.blank";
    type Settings = HashMap<String, String>;

    async fn will_disappear(
        &self,
        instance: &Instance,
        _: &Self::Settings,
    ) -> OpenActionResult<()> {
        //cleanup
        let _ = instance.set_title(Some(""), None);
        let _ = instance.set_image(Some(TRANSPARENT_ICON.as_str()), None);

        Ok(())
    }

    async fn will_appear(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let mut columns = utils::VOLUME_APPLICATION_COLUMNS.lock().await;
        let column_key = instance.coordinates.column;

        // Skip column 0 as it's reserved TODO
        if column_key == 0 {
            return Ok(());
        }

        let column = match columns.get_mut(&column_key) {
            Some(col) => col,
            None => return Ok(()),
        };

        match instance.coordinates.row {
            0 => {
                update_header(instance, column).await;
                column.header_id = Some(instance.instance_id.clone());
            }
            1 | 2 => {
                if let Ok((upper_img, lower_img)) =
                    gfx::get_volume_bar_data_uri_split(column.vol_percent)
                {
                    let img;
                    if instance.coordinates.row == 1 {
                        column.upper_vol_btn_id = Some(instance.instance_id.clone());
                        img = upper_img;
                    } else {
                        column.lower_vol_btn_id = Some(instance.instance_id.clone());
                        img = lower_img;
                    };
                    instance.set_image(Some(img), None).await?;
                }
            }
            _ => {} // Ignore other rows
        }

        Ok(())
    }

    async fn key_down(&self, instance: &Instance, _: &Self::Settings) -> OpenActionResult<()> {
        let mut columns = utils::VOLUME_APPLICATION_COLUMNS.lock().await;
        let column_key = instance.coordinates.column;

        if let Some(column) = columns.get_mut(&column_key) {
            match instance.coordinates.row {
                0 => {
                    println!("Muting app {}", column.name);
                    column.mute = !column.mute;
                    let mut audio_system = audio::create_audio_system();
                    audio_system.mute_volume(column.uid, column.mute).unwrap();
                }
                1 => {
                    let app_uid = column.uid;

                    if column.vol_percent >= 100.0 {
                        return Ok(());
                    }

                    {
                        let mut audio_system = audio::create_audio_system();
                        audio_system
                            .increase_volume(app_uid, VOLUME_INCREMENT as f64)
                            .unwrap();
                    }
                    column.vol_percent =
                        (column.vol_percent + VOLUME_INCREMENT_PERCENTAGE).clamp(0.0, 100.0);

                    if let Ok((upper_img, lower_img)) =
                        gfx::get_volume_bar_data_uri_split(column.vol_percent)
                    {
                        instance.set_image(Some(upper_img), None).await?;
                        if let Some(vol_btn_id) = column.lower_vol_btn_id.clone() {
                            let Some(related_instance) = get_instance(vol_btn_id).await else {
                                return Ok(());
                            };

                            related_instance.set_image(Some(lower_img), None).await?;
                        }
                    }

                    println!("Volume up app {} {}", column.name, column.vol_percent);
                }
                2 => {
                    let app_uid = column.uid;
                    {
                        let mut audio_system = audio::create_audio_system();
                        audio_system
                            .decrease_volume(app_uid, VOLUME_INCREMENT as f64)
                            .unwrap();
                    }
                    column.vol_percent =
                        (column.vol_percent - VOLUME_INCREMENT_PERCENTAGE).clamp(0.0, 100.0);

                    if let Ok((upper_img, lower_img)) =
                        gfx::get_volume_bar_data_uri_split(column.vol_percent)
                    {
                        instance.set_image(Some(lower_img), None).await?;
                        if let Some(vol_btn_id) = column.upper_vol_btn_id.clone() {
                            let Some(related_instance) = get_instance(vol_btn_id).await else {
                                return Ok(());
                            };

                            related_instance.set_image(Some(upper_img), None).await?;
                        }
                    }

                    println!("Volume down app {} {}", column.name, column.vol_percent);
                }
                _ => {}
            }
        }

        Ok(())
    }
}

pub async fn init() -> OpenActionResult<()> {
    println!("Stream Deck connected - starting PulseAudio monitoring");
    // start listening to changes
    pulse_monitor::start_pulse_monitoring();

    // create initial map
    let applications = {
        let mut audio_system = create_audio_system();
        audio_system
            .list_applications()
            .expect("Error fetching applications from SinkController")
    };
    utils::create_application_volume_columns(applications).await;

    register_action(VCAction).await;

    run(std::env::args().collect()).await
}

use crate::{
    audio,
    gfx::{self, BarPosition},
    switch_profile, utils,
};
use openaction::*;

pub struct ActionEventHandler {}

const VOLUME_INCREMENT: f32 = 0.1;
const VOLUME_INCREMENT_PERCENTAGE: f32 = VOLUME_INCREMENT * 100.0;

impl openaction::ActionEventHandler for ActionEventHandler {
    async fn will_appear(
        &self,
        event: AppearEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        let mut columns = utils::VOLUME_APPLICATION_COLUMNS.lock().await;
        let column_key = event.payload.coordinates.column;

        // Skip column 0 as it's reserved
        if column_key == 0 {
            return Ok(());
        }

        let column = match columns.get_mut(&column_key) {
            Some(col) => col,
            None => return Ok(()),
        };

        // Set the context based on the row
        match event.payload.coordinates.row {
            0 => {
                column.header_context = event.context.clone();
                outbound
                    .set_title(
                        column.header_context.clone(),
                        Some(column.app_name.clone()),
                        Some(0),
                    )
                    .await?;
            }
            1 => {
                column.volume_up_context = event.context.clone();
                let img = gfx::get_volume_bar_data_uri_split(
                    column.volume_percentage,
                    BarPosition::Upper,
                )?;
                outbound
                    .set_image(column.volume_up_context.clone(), Some(img), Some(0))
                    .await?;
            }
            2 => {
                column.volume_down_context = event.context.clone();
                let img = gfx::get_volume_bar_data_uri_split(
                    column.volume_percentage,
                    BarPosition::Lower,
                )?;
                outbound
                    .set_image(column.volume_down_context.clone(), Some(img), Some(0))
                    .await?;
            }
            _ => {} // Ignore other rows
        }

        println!("EEEEVAAAA WIL APP {:?}", event);

        Ok(())
    }

    async fn key_up(
        &self,
        _event: KeyEvent,
        _outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        Ok(())
    }

    async fn key_down(
        &self,
        event: KeyEvent,
        outbound: &mut OutboundEventManager,
    ) -> EventHandlerResult {
        println!("ACTION: {:?}", event.action);
        match &event.action[..] {
            "com.victormarin.volume-controller.back-to-profile" => {
                println!("I AM DEFINITELY HERE");
                utils::clear_screen(outbound).await.unwrap();
                switch_profile::run(outbound, "Test".to_string())
                    .await
                    .unwrap();
                Ok(())
            }
            "com.victormarin.volume-controller.auto-detection" => {
                println!("I AM CONNECTED");
                let applications = {
                    let mut audio_system = audio::create_audio_system();
                    audio_system
                        .list_applications()
                        .expect("Error fetching applications from SinkController")
                };
                utils::create_application_volume_columns(applications).await;
                switch_profile::run(outbound, "Sound".to_string())
                    .await
                    .unwrap();
                Ok(())
            }
            "com.victormarin.volume-controller.manual-detection" => {
                println!("THIS IS A MANUAL REFRESH");
                utils::clear_screen(outbound).await.unwrap();
                let applications = {
                    let mut audio_system = audio::create_audio_system();
                    audio_system
                        .list_applications()
                        .expect("Error fetching applications from SinkController")
                };
                utils::create_application_volume_columns(applications).await;
                switch_profile::run(outbound, "Sound".to_string())
                    .await
                    .unwrap();
                Ok(())
            }
            "com.victormarin.volume-controller.auto-detection.blank" => {
                let mut columns = utils::VOLUME_APPLICATION_COLUMNS.lock().await;
                let column_key = event.payload.coordinates.column;

                if let Some(column) = columns.get_mut(&column_key) {
                    match event.payload.coordinates.row {
                        0 => {
                            println!("Muting app {}", column.app_name);
                            column.app_mute = !column.app_mute;
                            let mut audio_system = audio::create_audio_system();
                            audio_system
                                .mute_volume(column.app_uid, column.app_mute)
                                .unwrap();
                        }
                        1 => {
                            let app_uid = column.app_uid;
                            {
                                let mut audio_system = audio::create_audio_system();
                                audio_system
                                    .increase_volume(app_uid, VOLUME_INCREMENT as f64)
                                    .unwrap();
                            }
                            column.volume_percentage = (column.volume_percentage
                                + VOLUME_INCREMENT_PERCENTAGE)
                                .clamp(0.0, 100.0);
                            let upper_img = gfx::get_volume_bar_data_uri_split(
                                column.volume_percentage,
                                BarPosition::Upper,
                            )?;
                            let lower_img = gfx::get_volume_bar_data_uri_split(
                                column.volume_percentage,
                                BarPosition::Lower,
                            )?;
                            outbound
                                .set_image(
                                    column.volume_up_context.clone(),
                                    Some(upper_img),
                                    Some(0),
                                )
                                .await?;
                            outbound
                                .set_image(
                                    column.volume_down_context.clone(),
                                    Some(lower_img),
                                    Some(0),
                                )
                                .await?;
                            println!(
                                "Volume up app {} {}",
                                column.app_name, column.volume_percentage
                            );
                        }
                        2 => {
                            let app_uid = column.app_uid;
                            {
                                let mut audio_system = audio::create_audio_system();
                                audio_system
                                    .decrease_volume(app_uid, VOLUME_INCREMENT as f64)
                                    .unwrap();
                            }
                            column.volume_percentage = (column.volume_percentage
                                - VOLUME_INCREMENT_PERCENTAGE)
                                .clamp(0.0, 100.0);
                            let upper_img = gfx::get_volume_bar_data_uri_split(
                                column.volume_percentage,
                                BarPosition::Upper,
                            )?;
                            let lower_img = gfx::get_volume_bar_data_uri_split(
                                column.volume_percentage,
                                BarPosition::Lower,
                            )?;
                            outbound
                                .set_image(
                                    column.volume_up_context.clone(),
                                    Some(upper_img),
                                    Some(0),
                                )
                                .await?;
                            outbound
                                .set_image(
                                    column.volume_down_context.clone(),
                                    Some(lower_img),
                                    Some(0),
                                )
                                .await?;
                            println!(
                                "Volume down app {} {}",
                                column.app_name, column.volume_percentage
                            );
                        }
                        _ => {}
                    }
                }

                Ok(())
            }
            _ => Ok(()),
        }
    }
}

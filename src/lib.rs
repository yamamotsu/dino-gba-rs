// Games made using `agb` are no_std which means you don't have access to the standard
// rust library. This is because the game boy advance doesn't really have an operating
// system, so most of the content of the standard library doesn't apply.
//
// Provided you haven't disabled it, agb does provide an allocator, so it is possible
// to use both the `core` and the `alloc` built in crates.
#![no_std]
// This is required to allow writing tests
#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]
#![cfg_attr(test, test_runner(agb::test_runner::test_runner))]

extern crate alloc;

use agb::{
    display::{
        tiled::{InfiniteScrolledMap, RegularBackgroundSize, TileFormat},
        Priority,
    },
    fixnum::num,
    mgba::Mgba,
    save::{Error, SaveData},
    sound::mixer::Frequency,
};
use alloc::boxed::Box;
use constant::{MAX_JUMP_DURATION_FRAMES, MAX_JUMP_HEIGHT_PX};
use game::{
    resource::{
        create_tile_map, BG_BLANK_TILE_IDX, BG_PALETTES, BG_TILES_DATA, BG_TILES_HEIGHT,
        BG_TILES_OFFSET_Y,
    },
    Game, GameState, Settings, SpriteCache,
};
use save::SaveBuffer;
use utils::print_info;

mod game;
mod save;
mod utils;

pub mod constant {
    // GamePlay Config
    pub const MAX_JUMP_HEIGHT_PX: u16 = 45;
    pub const MAX_JUMP_DURATION_FRAMES: u16 = 16;
    pub const BIRD_SPAWN_INTERVAL_FRAMES: u16 = 60 * 5;
    pub const CACTUS_SPAWN_INTERVAL_FRAMES: u16 = 60 * 3;
    pub const LEVEL_UP_INTERVAL_FRAMES: u16 = 60 * 30;
}

pub fn save(save_access: &mut SaveData, save_buffer: SaveBuffer) -> Result<(), Error> {
    let mut writer = save_access.prepare_write(0..5)?;
    writer.write(0, &save_buffer.as_array())
}

pub fn main(mut gba: agb::Gba) -> ! {
    let mut mgba = Mgba::new();
    let (mut oam, mut sprite_loader) = gba.display.object.get_unmanaged();
    let sprite_cache = SpriteCache::new(&mut sprite_loader);

    let (bg_graphics, mut vram) = gba.display.video.tiled0();
    vram.set_background_palettes(BG_PALETTES);

    let tile_map = create_tile_map();
    let mut background = InfiniteScrolledMap::new(
        bg_graphics.background(
            Priority::P0,
            RegularBackgroundSize::Background64x32,
            TileFormat::FourBpp,
        ),
        Box::new(|pos| {
            let x = pos.x.rem_euclid(64) as u16;
            let y = pos.y.rem_euclid(20) as u16;

            let tile_idx = if y >= BG_TILES_OFFSET_Y && y < BG_TILES_OFFSET_Y + BG_TILES_HEIGHT {
                *tile_map
                    .get((x + 64 * (y - BG_TILES_OFFSET_Y)) as usize)
                    .unwrap_or(&(BG_BLANK_TILE_IDX as usize)) as usize
            } else {
                BG_BLANK_TILE_IDX as usize
            };
            (&BG_TILES_DATA.tiles, BG_TILES_DATA.tile_settings[tile_idx])
        }),
    );

    background.init(&mut vram, (0, 0).into(), &mut || {});
    background.show();
    background.commit(&mut vram);

    let mut mixer = gba.mixer.mixer(Frequency::Hz10512);
    mixer.enable();

    gba.save.init_sram();
    let mut save_access = gba.save.access().unwrap();
    let mut save_buffer = SaveBuffer::new();
    save_access.read(0, save_buffer.as_mut_array()).unwrap();
    print_info(
        &mut mgba,
        format_args!("[init] saved data: {:?}", save_buffer),
    );

    let mut hi_score = if save_buffer.is_savedata_exist() == false {
        print_info(
            &mut mgba,
            format_args!("[init] initializing hi score save slot..."),
        );
        let result = save(&mut save_access, SaveBuffer::new());
        if result.is_err() {
            print_info(
                &mut mgba,
                format_args!("[ERR] failed to write: {:?}", result.unwrap_err()),
            );
        }
        0
    } else {
        save_buffer.get_score()
    };

    let vblank = agb::interrupt::VBlank::get();

    loop {
        let mut game = Game::from_settings(Settings {
            init_scroll_velocity: num!(3.4),
            jump_height_px: MAX_JUMP_HEIGHT_PX,
            jump_duration_frames: MAX_JUMP_DURATION_FRAMES,
            max_enemies_displayed: 3,
            spawn_interval_frames: 60,
            animation_interval_frames: 10,
            scroll_velocity_increase_per_level: num!(0.15),
            frames_to_level_up: 60 * 30,
            hi_score,
        });

        loop {
            let state = game.frame(&sprite_cache, &mut vram, &mut background, &mut mixer);
            mixer.frame();

            vblank.wait_for_vblank();
            let oam_frame = &mut oam.iter();
            game.render(oam_frame, &sprite_cache);
            background.commit(&mut vram);

            match state {
                GameState::Over(score) => {
                    if score > hi_score {
                        print_info(
                            &mut mgba,
                            format_args!("Hi score beat: {} -> {}", hi_score, score),
                        );
                        hi_score = score;
                        let result = save(&mut save_access, hi_score.into());
                        if result.is_err() {
                            print_info(
                                &mut mgba,
                                format_args!("[ERR] failed to write: {:?}", result.unwrap_err()),
                            );
                        }
                    }
                }
                GameState::Restart => {
                    print_info(&mut mgba, format_args!("Restarting.."));
                    break;
                }
                _ => {}
            };
        }
    }
}

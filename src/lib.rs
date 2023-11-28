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
        object::{Graphics, Tag},
        tile_data::TileData,
        tiled::{InfiniteScrolledMap, RegularBackgroundSize, TileFormat, TileSet, TileSetting},
        Priority,
    },
    fixnum::{num, Num, Rect, Vector2D},
    include_aseprite,
    input::Button,
    mgba::{DebugLevel, Mgba},
    rng,
};
use alloc::{boxed::Box, vec::Vec};

// Load background tiles as `bg_tiles` module
agb::include_background_gfx!(bg_tiles, tiles => "assets/gfx/dino_background.bmp");
const TILE_MAP_CSV_STR: &str = include_str!("../assets/tilemap/dino_map.csv");
const BG_TILE_DATA: TileData = bg_tiles::tiles;
const BG_TILES: TileSet = BG_TILE_DATA.tiles;
const BG_TILE_CONFIG: &[TileSetting] = BG_TILE_DATA.tile_settings;

const SPRITES: &Graphics = include_aseprite!("assets/gfx/dino.aseprite");

// We define some easy ways of referencing the sprites
const DINO: &Tag = SPRITES.tags().get("Dino");
const BIRD: &Tag = SPRITES.tags().get("Bird");
const CACTUS: &Tag = SPRITES.tags().get("Cactus");
const SPRITE_ANIMATION_DELAY_FRAMES: u32 = 8;

// Map Config
const MAP_TILES_HEIGHT: u16 = 14;
const MAP_TILES_OFFSET_Y: u16 = (20 - MAP_TILES_HEIGHT) / 2;
const BLANK_TILE_IDX: u16 = 1;

const GROUND_TILE_Y: u16 = 11 + MAP_TILES_OFFSET_Y;
const GROUND_Y: u16 = GROUND_TILE_Y * 8 + 2;
const DINO_GROUNDED_Y: u16 = GROUND_Y - 32;
const CACTUS_Y: u16 = GROUND_Y - 32;

// GamePlay Config
const MAX_JUMP_HEIGHT_PX: u16 = 40;
const MAX_JUMP_DURATION_FRAMES: u16 = 16;
const BIRD_SPAWN_INTERVAL_FRAMES: u16 = 60 * 5;
const CACTUS_SPAWN_INTERVAL_FRAMES: u16 = 60 * 3;
const LEVEL_UP_INTERVAL_FRAMES: u16 = 60 * 30;

fn frame_ranger(count: u32, start: u32, end: u32, delay: u32) -> usize {
    (((count / delay) % (end + 1 - start)) + start) as usize
}
fn print_info(mgba: &mut Mgba, output: core::fmt::Arguments) {
    // Debug output
    mgba.print(output, DebugLevel::Info).unwrap();
}

pub fn main(mut gba: agb::Gba) -> ! {
    let mut input = agb::input::ButtonController::new();
    let mut mgba: agb::mgba::Mgba = agb::mgba::Mgba::new().unwrap();
    let vblank = agb::interrupt::VBlank::get();

    // Debug output
    print_info(
        &mut mgba,
        format_args!("Tile format: {:?}", BG_TILES.format()),
    );
    print_info(&mut mgba, format_args!("Tile config: {:?}", BG_TILE_CONFIG));
    for color_idx in 0..16 {
        print_info(
            &mut mgba,
            format_args!(
                "PALETTE {color_idx:02}: {:b}",
                bg_tiles::PALETTES[0].colour(color_idx)
            ),
        );
    }

    // Physics
    let gravity_px_per_square_frame: Num<i32, 8> =
        Num::new(2 * MAX_JUMP_HEIGHT_PX as i32) / Num::new(MAX_JUMP_DURATION_FRAMES.pow(2) as i32);
    print_info(
        &mut mgba,
        format_args!("Gravity: {:?} px/frame^2", gravity_px_per_square_frame),
    );

    // Background Initialization
    //  1. Get Tiled mode graphics manager
    let (gfx, mut vram) = gba.display.video.tiled0();
    //  2. load TileMap (tile indices of each grid position)
    let tile_map: Vec<usize> = TILE_MAP_CSV_STR
        .splitn(64 * 32, [',', '\r', '\n'])
        .map(|s| usize::from_str_radix(s, 10).unwrap_or(0))
        .collect();
    //  3. set bg palettes
    vram.set_background_palettes(bg_tiles::PALETTES);
    //  4. create infinite scrolled background
    let mut background = InfiniteScrolledMap::new(
        gfx.background(
            Priority::P0,
            RegularBackgroundSize::Background64x32,
            TileFormat::FourBpp,
        ),
        Box::new(|pos| {
            let x = pos.x.rem_euclid(64) as u16;
            let y = pos.y.rem_euclid(20) as u16;
            let tile_idx = if y >= MAP_TILES_OFFSET_Y && y < MAP_TILES_OFFSET_Y + MAP_TILES_HEIGHT {
                *tile_map
                    .get((x + 64 * (y - MAP_TILES_OFFSET_Y)) as usize)
                    .unwrap_or(&0) as usize
            } else {
                BLANK_TILE_IDX as usize
            };
            (&BG_TILES, BG_TILE_CONFIG[tile_idx])
        }),
    );
    background.init(&mut vram, (0, 0).into(), &mut || {});

    background.set_pos(&mut vram, (0, 0).into());
    background.commit(&mut vram);
    background.show();

    // Objects
    let object = gba.display.object.get_managed();
    let mut dino = object.object_sprite(DINO.sprite(0));
    let mut bird = object.object_sprite(BIRD.sprite(0));
    let mut cactus = object.object_sprite(CACTUS.sprite(0));

    let mut is_game_over: bool = false;
    let mut frame_count: u32 = 0;
    let mut position: Vector2D<Num<i32, 8>> = (0, 0).into();
    let mut scroll_velocity: Num<i32, 8> = num!(2.5);
    let mut speed_level: u32 = 0;
    let mut t_last_level_up: u32 = 0;

    let mut bird_shown: bool = false;
    let mut t_last_bird_spawned: u32 = 0;
    let mut bird_position: Vector2D<Num<i32, 8>> = (0, 0).into();
    let bird_velocity: Num<i32, 8> = num!(-0.4);

    let dino_x: u16 = 16;
    let mut dino_y: u16 = DINO_GROUNDED_Y;
    let mut dino_velocity_y: Num<i32, 8> = Num::new(0);
    let mut dino_grounded: bool = true;

    let mut cactus_shown: bool = false;
    let mut cactus_position: Vector2D<Num<i32, 8>> = (0, 0).into();
    let mut t_last_cactus_spawned: u32 = 0;

    dino.set_x(dino_x).set_y(dino_y).show();
    object.commit();

    loop {
        if is_game_over {
            continue;
        }

        if dino_grounded {
            dino.set_sprite(object.sprite(DINO.sprite(frame_ranger(
                frame_count,
                0,
                1,
                SPRITE_ANIMATION_DELAY_FRAMES,
            ))));

            if input.is_just_pressed(Button::A) {
                dino_velocity_y = -gravity_px_per_square_frame * (MAX_JUMP_DURATION_FRAMES as i32);
                // print_info(&mut mgba, format_args!("jump up velocity: {:?}", dino_velocity_y));
                dino.set_sprite(object.sprite(DINO.sprite(1)));
                dino_grounded = false;
            };
        } else {
            // print_info(&mut mgba, format_args!("jumping velocity: {:?}", dino_velocity_y.floor()));
            dino_y = (dino_y as i32 + dino_velocity_y.floor()) as u16;
            if dino_y >= DINO_GROUNDED_Y {
                dino_y = DINO_GROUNDED_Y;
                dino_grounded = true;
            };
            dino.set_y(dino_y);
            dino_velocity_y += gravity_px_per_square_frame;
        };

        position.x += scroll_velocity;
        background.set_pos(&mut vram, position.floor());

        if frame_count - t_last_level_up > LEVEL_UP_INTERVAL_FRAMES as u32 {
            speed_level += 1;
            t_last_level_up = frame_count;
            scroll_velocity += num!(0.1);
            print_info(
                &mut mgba,
                format_args!("lvl up -> {}, V={:2}", speed_level + 1, scroll_velocity),
            );
        };

        if bird_shown {
            bird.set_sprite(object.sprite(BIRD.sprite(frame_ranger(
                frame_count,
                0,
                1,
                SPRITE_ANIMATION_DELAY_FRAMES,
            ))));

            bird_position.x += bird_velocity - scroll_velocity;
            let bird_position_int = bird_position.floor();
            bird.set_position(bird_position_int);
            if bird_position_int.x >= dino_x as i32 && bird_position_int.x <= dino_x as i32 + 32 {
                let dino_rect: Rect<i32> = Rect::new((dino_x, dino_y).into(), (32, 32).into());
                let bird_rect: Rect<i32> = Rect::new(
                    (bird_position_int.x, bird_position_int.y + 12).into(),
                    (32, 9).into(),
                );
                if dino_rect.touches(bird_rect) {
                    // Game Over
                    dino.set_sprite(object.sprite(DINO.sprite(2)));
                    is_game_over = true;
                };
            } else if bird_position_int.x < -32 {
                bird.hide();
                bird_shown = false;
            };
        } else if frame_count - t_last_bird_spawned > BIRD_SPAWN_INTERVAL_FRAMES as u32 {
            t_last_bird_spawned = frame_count;
            // Spawn bird
            let spawn_y: i32 = ((rng::gen() & 0b0011) + 2) * 16;
            bird_position.y = Num::new(spawn_y);
            bird_position.x = Num::new(8 * 30);
            bird_shown = true;
            bird.set_position(bird_position.floor());
            bird.show();
        };

        if cactus_shown {
            cactus_position.x -= scroll_velocity;
            let cactus_position_int = cactus_position.floor();
            cactus.set_position(cactus_position_int);

            if cactus_position_int.x >= dino_x as i32 && cactus_position_int.x <= dino_x as i32 + 32
            {
                let dino_rect: Rect<i32> = Rect::new((dino_x, dino_y).into(), (32, 32).into());
                let cactus_rect: Rect<i32> = Rect::new(
                    (cactus_position_int.x + 3, cactus_position_int.y + 2).into(),
                    (26, 30).into(),
                );
                if dino_rect.touches(cactus_rect) {
                    // Game Over
                    dino.set_sprite(object.sprite(DINO.sprite(2)));
                    is_game_over = true;
                };
            }
            if cactus_position_int.x < -32 {
                cactus.hide();
                cactus_shown = false;
            };
        } else if frame_count - t_last_cactus_spawned > CACTUS_SPAWN_INTERVAL_FRAMES as u32 {
            t_last_cactus_spawned = frame_count;
            cactus_position.y = Num::new(CACTUS_Y as i32);
            cactus_position.x = Num::new(8 * 30);
            cactus_shown = true;
            cactus.set_position(cactus_position.floor());
            cactus.show();
        };

        // Wait for vblank, then commit the objects to the screen
        vblank.wait_for_vblank();
        input.update();
        object.commit();
        background.commit(&mut vram);
        frame_count += 1;
    }
}

use core::ops::Range;

use agb::{
    display::{
        object::{OamIterator, ObjectUnmanaged, SpriteLoader, SpriteVram, Tag},
        tiled::{InfiniteScrolledMap, VRamManager},
    },
    fixnum::{Num, Rect, Vector2D},
    input::{Button, ButtonController},
    mgba::Mgba,
};
use alloc::{boxed::Box, collections::VecDeque, vec::Vec};

pub type Number = Num<i32, 8>;

pub mod resource {
    use agb::display::{
        object::{Graphics, Sprite, Tag},
        palette16::Palette16,
        tile_data::TileData,
    };
    use alloc::vec::Vec;

    const SPRITES: &Graphics = agb::include_aseprite!("assets/gfx/dino.aseprite");
    pub(super) const DINO: &Tag = SPRITES.tags().get("Dino");
    pub(super) const BIRD: &Tag = SPRITES.tags().get("Bird");
    pub(super) const CACTUS: &Sprite = SPRITES.tags().get("Cactus").sprite(0);

    // Load background tiles as `bg_tiles` module
    agb::include_background_gfx!(bg_tiles, tiles => "assets/gfx/dino_background.bmp");
    const TILE_MAP_CSV_STR: &str = include_str!("../assets/tilemap/dino_map.csv");

    pub const BG_TILES_DATA: TileData = bg_tiles::tiles;
    pub const BG_PALETTES: &[Palette16] = bg_tiles::PALETTES;

    pub fn create_tile_map() -> Vec<usize> {
        TILE_MAP_CSV_STR
            .split([',', '\r', '\n'])
            .map(|s| usize::from_str_radix(s, 10).unwrap_or(0))
            .collect()
    }

    pub const BG_TILES_WIDTH: u16 = 64;
    pub const BG_TILES_HEIGHT: u16 = 14;
    pub const BG_TILES_OFFSET_Y: u16 = (20 - BG_TILES_HEIGHT) / 2;
    pub const BG_BLANK_TILE_IDX: u16 = 1;
    pub const GROUND_TILE_Y: u16 = 11 + BG_TILES_OFFSET_Y;
    pub const GROUND_Y: u16 = GROUND_TILE_Y * 8 + 2;

    pub const DINO_GROUNDED_Y: u16 = GROUND_Y - 32;
    pub const CACTUS_Y: u16 = GROUND_Y - 32;
}

use crate::utils::print_info;

use self::resource::{BIRD, CACTUS, CACTUS_Y, DINO, DINO_GROUNDED_Y};

#[derive(Clone)]
pub struct SpriteCache {
    dino: Box<[SpriteVram]>,
    bird: Box<[SpriteVram]>,
    cactus: SpriteVram,
}

impl SpriteCache {
    pub fn new(loader: &mut SpriteLoader) -> Self {
        fn generate_sprites(
            tag: &'static Tag,
            range: Range<usize>,
            loader: &mut SpriteLoader,
        ) -> Box<[SpriteVram]> {
            range
                .map(|x| tag.sprite(x))
                .map(|x| loader.get_vram_sprite(x))
                .collect::<Vec<_>>()
                .into_boxed_slice()
        }

        Self {
            dino: generate_sprites(DINO, 0..3, loader),
            bird: generate_sprites(BIRD, 0..2, loader),
            cactus: loader.get_vram_sprite(CACTUS),
        }
    }
}

struct Player {
    sprites: Box<[SpriteVram]>,
    position: Vector2D<Number>,
    vertical_speed: Number,

    is_jumping: bool,
}

enum EnemyKind {
    Bird,
    Cactus,
}
struct Enemy {
    kind: EnemyKind,
    position: Vector2D<Number>,
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub init_scroll_velocity: Number,

    pub scroll_velocity_increase_per_level: Number,
    pub frames_to_level_up: u32,

    pub animation_interval_frames: u16,
    pub spawn_interval_frames: u16,
    pub jump_height_px: u16,
    pub jump_duration_frames: u16,
    pub max_enemies_displayed: usize,
}

pub enum GameState {
    Continue,
    Over,
}

pub struct Game {
    mgba: Mgba,
    settings: Settings,
    frame_count: u32,
    speed_level: u16,
    background_position: Vector2D<Number>,
    scroll_velocity: Number,
    gravity_px_per_square_frame: Number,
    input: ButtonController,
    player: Player,
    enemies: VecDeque<Enemy>,
    frames_current_level: u32,
    frames_since_last_spawn: u32,
}

fn frame_ranger(count: u32, start: u32, end: u32, delay: u32) -> usize {
    (((count / delay) % (end + 1 - start)) + start) as usize
}

impl Game {
    pub fn from_settings(settings: Settings, sprite_cache: &SpriteCache) -> Self {
        let player = Player {
            sprites: sprite_cache.dino.clone(),
            position: (16, DINO_GROUNDED_Y as i32).into(),
            vertical_speed: Number::new(0),
            is_jumping: false,
        };
        let gravity_px_per_square_frame: Number = Number::new(2 * settings.jump_height_px as i32)
            / Number::new(settings.jump_duration_frames.pow(2) as i32);

        Self {
            mgba: Mgba::new().unwrap(),
            frame_count: 0,
            frames_current_level: 0,
            frames_since_last_spawn: 0,
            speed_level: 0,
            background_position: (0, 0).into(),
            scroll_velocity: settings.init_scroll_velocity,
            input: agb::input::ButtonController::new(),
            player,
            enemies: VecDeque::with_capacity(settings.max_enemies_displayed),
            gravity_px_per_square_frame,
            settings,
        }
    }

    pub fn frame(
        &mut self,
        sprite_cache: &SpriteCache,
        vram: &mut VRamManager,
        background: &mut InfiniteScrolledMap<'_>,
    ) -> GameState {
        self.input.update();
        self.frame_count += 1;
        self.frames_current_level += 1;
        self.frames_since_last_spawn += 1;

        // Calc player position
        if self.player.is_jumping {
            self.player.position.y += self.player.vertical_speed;
            let player_y_px = self.player.position.y.floor();
            if player_y_px >= DINO_GROUNDED_Y as i32 {
                self.player.position.y = Num::new(DINO_GROUNDED_Y as i32);
                self.player.is_jumping = false;
            }
            self.player.vertical_speed += self.gravity_px_per_square_frame;
        } else if self.input.is_just_pressed(Button::A) {
            self.player.vertical_speed =
                -self.gravity_px_per_square_frame * (self.settings.jump_duration_frames as i32);
            self.player.is_jumping = true;
        }

        // Spawn enemy
        if self.frames_since_last_spawn > self.settings.spawn_interval_frames as u32 {
            self.frames_since_last_spawn = 0;

            if self.enemies.len() < self.enemies.capacity() {
                let rnd = agb::rng::gen();
                let spawn_check: bool = (rnd & 0b11) == 0; // 25% spawn
                print_info(
                    &mut self.mgba,
                    format_args!("spawn?: {} {:b}", spawn_check, rnd & 0xFF),
                );
                if spawn_check == true {
                    let enemy_selection = (rnd & 0b11100) >> 2;
                    let enemy = if enemy_selection < 3 {
                        // choose spawn position
                        let spawn_y = (((rnd & 0b1100000) >> 5) + 6) * 8;
                        Enemy {
                            kind: EnemyKind::Bird,
                            position: (8 * 30, spawn_y).into(),
                        }
                    } else {
                        Enemy {
                            kind: EnemyKind::Cactus,
                            position: (8 * 30, CACTUS_Y as i32).into(),
                        }
                    };
                    self.enemies.push_back(enemy);
                }
            }
        }

        // Calc enemies' position
        let mut total_enemies_out: usize = 0;
        for enemy in self.enemies.iter_mut() {
            if enemy.position.x.floor() < -32 {
                total_enemies_out += 1;
            } else {
                enemy.position.x -= self.scroll_velocity;
            };
        }
        // Pop first n enemies which are out of screen
        if total_enemies_out > 0 {
            print_info(
                &mut self.mgba,
                format_args!("remove enemies: {}", total_enemies_out),
            );
        }
        self.enemies.drain(..total_enemies_out);

        // Process level up
        if self.frames_current_level >= self.settings.frames_to_level_up {
            print_info(
                &mut self.mgba,
                format_args!("level up: {}", self.speed_level + 1),
            );
            self.scroll_velocity += self.settings.scroll_velocity_increase_per_level;
            self.speed_level += 1;
            self.frames_current_level = 0;
        }

        self.background_position.x += self.scroll_velocity;
        background.set_pos(vram, self.background_position.floor());
        GameState::Continue
    }

    pub fn render(
        &mut self,
        oam_frame: &mut OamIterator,
        sprite_cache: &SpriteCache,
    ) -> Option<()> {
        let sprite_index: usize = frame_ranger(
            self.frame_count,
            0,
            1,
            self.settings.animation_interval_frames as u32,
        );

        let sprite = if self.player.is_jumping {
            self.player.sprites.get(1).unwrap()
        } else {
            self.player.sprites.get(sprite_index).unwrap()
        };
        let mut player_object = ObjectUnmanaged::new(sprite.clone());
        player_object
            .show()
            .set_position(self.player.position.floor());
        oam_frame.next()?.set(&player_object);

        for enemy in self.enemies.iter() {
            let sprite = match enemy.kind {
                EnemyKind::Bird => sprite_cache.bird.get(sprite_index).unwrap().clone(),
                EnemyKind::Cactus => sprite_cache.cactus.clone(),
            };
            let mut object = ObjectUnmanaged::new(sprite);
            object.show().set_position(enemy.position.floor());
            oam_frame.next()?.set(&object);
        }

        Some(())
    }
}

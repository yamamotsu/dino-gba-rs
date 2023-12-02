#[derive(Debug, Clone, Copy)]
pub struct SaveBuffer([u8; 5]);
impl SaveBuffer {
    pub fn new() -> Self {
        Self([0, 0, 0, 0, 0])
    }

    pub fn as_mut_array(&mut self) -> &mut [u8] {
        &mut self.0
    }
    pub fn as_array(&self) -> &[u8] {
        &self.0
    }

    pub fn is_savedata_exist(&self) -> bool {
        self.0[0] == 0
    }

    pub fn get_score(&self) -> u32 {
        self.0[1..]
            .into_iter()
            .enumerate()
            .fold(0, |acc, (index, byte)| {
                acc | ((*byte as u32) << (index * 8))
            })
    }
}

impl From<u32> for SaveBuffer {
    fn from(value: u32) -> Self {
        let mut arr: [u8; 5] = [0, 0, 0, 0, 0];
        for (index, byte) in value.to_le_bytes().iter().enumerate() {
            arr[index + 1] = *byte;
        }
        Self(arr)
    }
}
impl From<[u8; 5]> for SaveBuffer {
    fn from(value: [u8; 5]) -> Self {
        Self(value)
    }
}

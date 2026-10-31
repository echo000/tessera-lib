use porter_texture::Image;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NormalMapEncoding {
    Rgb,
    ReconstructZ,
}

#[derive(Debug, Clone)]
pub struct NormalMap {
    pub image: Image,
    pub encoding: NormalMapEncoding,
    pub strength: f32,
    pub invert_y: bool,
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialBlend {
    Opaque,
    Alpha,
    Additive,
}

#[derive(Debug, Clone, Copy)]
pub struct MaterialSettings {
    pub two_sided: bool,
    pub unlit: bool,
    pub alpha_test: bool,
    pub alpha_cutoff: f32,
    pub blend: MaterialBlend,
    pub clockwise: bool,
    pub vertex_color: bool,
    pub tint: [f32; 4],
}

impl Default for MaterialSettings {
    fn default() -> Self {
        Self {
            two_sided: false,
            unlit: false,
            alpha_test: false,
            alpha_cutoff: 0.5,
            blend: MaterialBlend::Opaque,
            clockwise: true,
            vertex_color: false,
            tint: [1.0; 4],
        }
    }
}

impl MaterialSettings {
    pub(crate) fn pipeline_index(self) -> usize {
        ((usize::from(!self.clockwise) * 2) + usize::from(self.two_sided)) * 3 + self.blend as usize
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelMaterial {
    pub image: Option<Image>,
    pub settings: MaterialSettings,
    #[cfg(feature = "normal-maps")]
    pub normal_map: Option<NormalMap>,
}

impl From<Option<Image>> for ModelMaterial {
    fn from(image: Option<Image>) -> Self {
        Self {
            image,
            settings: MaterialSettings::default(),
            #[cfg(feature = "normal-maps")]
            normal_map: None,
        }
    }
}

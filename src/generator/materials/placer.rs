use std::collections::HashMap;

use crate::{editor::{self, Editor}, generator::materials::{feature::MaterialParameters, Material, MaterialId}, geometry::Point3D, minecraft::BlockForm};


pub struct MaterialPlacer<'materials> {
    base_material: MaterialId,
    shade_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    wetness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    wear_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    decorativeness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    materials: &'materials HashMap<MaterialId, Material>,
}

impl<'materials> MaterialPlacer<'materials> {
    pub fn new(
        base_material: MaterialId,
        materials: &'materials HashMap<MaterialId, Material>,
    ) -> Self {
        MaterialPlacer {
            base_material,
            shade_function: None,
            wetness_function: None,
            wear_function: None,
            decorativeness_function: None,
            materials,
        }
    }

    pub fn with_shade_function(
        mut self,
        shade_function: impl Fn(Point3D) -> f32 + 'static,
    ) -> Self {
        self.shade_function = Some(Box::new(shade_function));
        self
    }

    pub fn with_wetness_function(
        mut self,
        wetness_function: impl Fn(Point3D) -> f32 + 'static,
    ) -> Self {
        self.wetness_function = Some(Box::new(wetness_function));
        self
    }

    pub fn with_wear_function(
        mut self,
        wear_function: impl Fn(Point3D) -> f32 + 'static,
    ) -> Self {
        self.wear_function = Some(Box::new(wear_function));
        self
    }

    pub fn with_decorativeness_function(
        mut self,
        decorativeness_function: impl Fn(Point3D) -> f32 + 'static,
    ) -> Self {
        self.decorativeness_function = Some(Box::new(decorativeness_function));
        self
    }

    pub async fn place_block(&self, editor: &mut Editor, point: Point3D, form: BlockForm) {
        let parameters = MaterialParameters {
            shade: self.shade_function.as_ref().map_or(0.5, |f| f(point)),
            wear: self.wear_function.as_ref().map_or(0.5, |f| f(point)),
            moisture: self.wetness_function.as_ref().map_or(0.5, |f| f(point)),
            decoration: self.decorativeness_function.as_ref().map_or(0.5, |f| f(point)),
        };

        if let Some(material) = self.materials.get(&self.base_material) {
            material.place_block(editor, point, form, self.materials, None, None, parameters).await;
        }
    }

    pub async fn place_blocks<Iter>(&self, editor: &mut Editor, points: Iter, form: BlockForm)
    where
        Iter: IntoIterator<Item = Point3D>,
    {
        for point in points {
            self.place_block(editor, point, form).await;
        }
    }

    pub fn with_form(self, form: BlockForm) -> MaterialPlacerWithForm<'materials> {
        MaterialPlacerWithForm::new(
            self.base_material,
            self.shade_function,
            self.wetness_function,
            self.wear_function,
            self.decorativeness_function,
            form,
            self.materials,
        )
    }
}

pub struct MaterialPlacerWithForm<'materials> {
    base_material: MaterialId,
    shade_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    wetness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    wear_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    decorativeness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    form: BlockForm,
    materials: &'materials HashMap<MaterialId, Material>,
}

impl<'materials> MaterialPlacerWithForm<'materials> {
    pub fn new(
        base_material: MaterialId,
        shade_function: Option<Box<dyn Fn(Point3D) -> f32>>,
        wetness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
        wear_function: Option<Box<dyn Fn(Point3D) -> f32>>,
        decorativeness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
        form: BlockForm,
        materials: &'materials HashMap<MaterialId, Material>,
    ) -> Self {
        MaterialPlacerWithForm {
            base_material,
            shade_function,
            wetness_function,
            wear_function,
            decorativeness_function,
            form,
            materials,
        }
    }

    pub async fn place_block(&self, editor: &mut Editor, point: Point3D) {
        let parameters = MaterialParameters {
            shade: self.shade_function.as_ref().map_or(0.5, |f| f(point)),
            wear: self.wear_function.as_ref().map_or(0.5, |f| f(point)),
            moisture: self.wetness_function.as_ref().map_or(0.5, |f| f(point)),
            decoration: self.decorativeness_function.as_ref().map_or(0.5, |f| f(point)),
        };
        if let Some(material) = self.materials.get(&self.base_material) {
            material.place_block(editor, point, self.form, self.materials, None, None, parameters).await;
        }
    }
}

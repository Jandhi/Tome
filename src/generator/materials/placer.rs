use std::collections::HashMap;

use crate::{editor::{Editor}, generator::materials::{feature::MaterialParameters, Material, MaterialId}, geometry::Point3D, minecraft::BlockForm};


pub struct Placer<'materials> {
    shade_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    wetness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    wear_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    decorativeness_function: Option<Box<dyn Fn(Point3D) -> f32>>,
    materials: &'materials HashMap<MaterialId, Material>,
}

impl<'materials> Placer<'materials> {
    pub fn new(
        materials: &'materials HashMap<MaterialId, Material>,
    ) -> Self {
        Placer {
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

    pub async fn place_block(&self, editor: &mut Editor, point: Point3D, material : &MaterialId, form: BlockForm, state : Option<&HashMap<String, String>>, data : Option<&String>) {
        let parameters = MaterialParameters {
            shade: self.shade_function.as_ref().map_or(0.5, |f| f(point)),
            wear: self.wear_function.as_ref().map_or(0.5, |f| f(point)),
            moisture: self.wetness_function.as_ref().map_or(0.5, |f| f(point)),
            decoration: self.decorativeness_function.as_ref().map_or(0.5, |f| f(point)),
        };

        if let Some(material) = self.materials.get(&material) {
            material.place_block(editor, point, form, self.materials, state, data, parameters).await;
        }
    }

    pub async fn place_blocks<Iter>(&self, editor: &mut Editor, points: Iter, material : &MaterialId, form: BlockForm, state : Option<&HashMap<String, String>>, data : Option<&String>)
    where
        Iter: IntoIterator<Item = Point3D>,
    {
        for point in points {
            self.place_block(editor, point, material, form, state.clone(), data.clone()).await;
        }
    }
}

pub struct MaterialPlacer<'materials> {
    placer: Placer<'materials>,
    material: MaterialId,
}

impl<'materials> MaterialPlacer<'materials> {
    pub fn new(placer: Placer<'materials>, material: MaterialId) -> Self {
        MaterialPlacer { placer, material }
    }

    pub async fn place_block(&self, editor: &mut Editor, point: Point3D, form: BlockForm, state: Option<&HashMap<String, String>>, data: Option<&String>) {
        self.placer.place_block(editor, point, &self.material, form, state, data).await;
    }

    pub async fn place_blocks<Iter>(&self, editor: &mut Editor, points: Iter, form: BlockForm, state: Option<&HashMap<String, String>>, data: Option<&String>)
    where
        Iter: IntoIterator<Item = Point3D>,
    {
        self.placer.place_blocks(editor, points, &self.material, form, state, data).await;
    }
}
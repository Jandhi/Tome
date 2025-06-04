use std::collections::HashMap;

use crate::{editor::{self, Editor}, generator::materials::{feature::MaterialParameters, Material, MaterialId}, geometry::Point3D, minecraft::BlockForm};



pub struct MaterialPlacer<'materials> {
    base_material : MaterialId,
    parameter_generator : Box<dyn Fn(Point3D) -> MaterialParameters>,
    materials : &'materials HashMap<MaterialId, Material>,
}

impl<'materials> MaterialPlacer<'materials> {
    pub fn new(base_material: MaterialId, parameter_generator: Box<dyn Fn(Point3D) -> MaterialParameters>, materials : &'materials HashMap<MaterialId, Material>) -> Self {
        MaterialPlacer {
            base_material,
            parameter_generator,
            materials
        }
    }

    pub async fn place_block(&self, editor: &mut Editor, point: Point3D, form : BlockForm) {
        let parameters = (self.parameter_generator)(point);
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
        MaterialPlacerWithForm::new(self.base_material, self.parameter_generator, form, self.materials)
    }
}

pub struct MaterialPlacerWithForm<'materials> {
    base_material : MaterialId,
    parameter_generator : Box<dyn Fn(Point3D) -> MaterialParameters>,
    form : BlockForm,
    materials : &'materials HashMap<MaterialId, Material>,
}

impl<'materials> MaterialPlacerWithForm<'materials> {
    pub fn new(base_material: MaterialId, parameter_generator: Box<dyn Fn(Point3D) -> MaterialParameters>, form : BlockForm, materials : &'materials HashMap<MaterialId, Material>) -> Self {
        MaterialPlacerWithForm {
            base_material,
            parameter_generator,
            form,
            materials
        }
    }

    pub async fn place_block(&self, editor: &mut Editor, point: Point3D) {
        let parameters = (self.parameter_generator)(point);
        if let Some(material) = self.materials.get(&self.base_material) {
            material.place_block(editor, point, self.form, self.materials, None, None, parameters).await;
        }
    }
}
use crate::geometry::Point3D;

pub fn point_to_chunk_coordinates(point: Point3D) -> Point3D {
    // seems off when negative but throughs and error if you -1 in neg case 
    Point3D {
        x: point.x >> 4,
        y: point.y >> 4,
        z: point.z >> 4,
    }
}

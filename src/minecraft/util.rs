use crate::geometry::Point3D;

pub fn point_to_chunk_coordinates(point: Point3D) -> Point3D {
    Point3D {
        x: (point.x / 16),
        y: (point.y / 16),
        z: (point.z / 16),
    }
}

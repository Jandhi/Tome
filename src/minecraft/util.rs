use crate::geometry::Point3D;

pub fn point_to_chunk_coordinates(point: Point3D) -> Point3D {
    // seems off when negative but throughs and error if you -1 in neg case 
    Point3D {
        x: if point.y >= 0 {(point.x / 16)} else {(point.x + 1 / 16) - 1},
        y: if point.y >= 0 {(point.y / 16)} else {(point.y + 1 / 16) - 1}, // Adjust for negative y
        z: if point.y >= 0 {(point.z / 16)} else {(point.z + 1 / 16) - 1},
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        generator::{
            nbts::Rotation,
            placement::{anchor_offset_for_rotation, footprint_dims_for_rotation},
        },
    };

    #[test]
    fn footprint_dims_no_rotation() {
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::None), (5, 3));
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::Twice), (5, 3));
    }

    #[test]
    fn footprint_dims_quarter_rotations() {
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::Once), (3, 5));
        assert_eq!(footprint_dims_for_rotation((5, 3), Rotation::Thrice), (3, 5));
    }

    #[test]
    fn anchor_offset_table_matches_plan() {
        let size = (5, 3);
        let origin_xz = (1, 2);

        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::None), (1, 2));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Once), (0, 1));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Twice), (3, 0));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Thrice), (2, 3));
    }

    #[test]
    fn anchor_offset_corner_origin() {
        // Origin at (0,0) — equivalent to "rect min corner is anchor".
        let size = (4, 6);
        let origin_xz = (0, 0);

        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::None), (0, 0));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Once), (5, 0));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Twice), (3, 5));
        assert_eq!(anchor_offset_for_rotation(size, origin_xz, Rotation::Thrice), (0, 3));
    }
}

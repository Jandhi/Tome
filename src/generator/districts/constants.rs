pub const CHUNK_SIZE: i32 = 16;
pub const SPAWN_PARCELS_RETRIES: i32 = 10;
pub const SPAWN_PARCELS_MIN_DISTANCE : i32 = 5;
pub const NUM_RECENTER : i32 = 2;
pub const TARGET_PARCEL_AMOUNT : u32 = 16; // Desired number of interior parcels; sets the target average size S = interior_blocks / TARGET_PARCEL_AMOUNT
pub const PARCEL_SIZE_LOWER_FACTOR : f32 = 0.5; // Interior parcels must be at least this fraction of the target average (band floor L = 0.5*S)
pub const PARCEL_SIZE_UPPER_FACTOR : f32 = 1.5; // Interior merges may not push a parcel above this fraction of the target average (band ceiling U = 1.5*S)
pub const OFF_LIMITS_ROUGHNESS : f32 = 6.0;
pub const OFF_LIMITS_GRADIENT : f32 = 1.0;
pub const URBAN_WATER_LIMIT: f32 = 0.33; // Maximum water percentage for urban parcels
pub const URBAN_SIZE_MIN: u32 = 3; // Minimum number of urban districts (city floor)
pub const URBAN_SIZE_MAX: u32 = 5; // Maximum number of urban districts (city cap)
pub const URBAN_GROWTH_CUTOFF: f32 = 0.10; // Candidate score needed to grow the city up to URBAN_SIZE_MIN
pub const URBAN_GROWTH_CUTOFF_HIGH: f32 = 0.33; // Higher candidate score needed to grow beyond the minimum, up to URBAN_SIZE_MAX
pub const URBAN_OPTION_SCORE_MAX: f32 = 0.75; // Max district_score to be eligible as an urban (prime) candidate
pub const RURAL_OPTION_SCORE_MAX: f32 = 1.5; // Max district_score to be classified Rural (above this is Off-Limits)
pub const ADJACENCY_WEIGHT: f32 = 3.0; // Weight for adjacency in parcel comparison scoring (district merge)
pub const CITY_GROWTH_ADJACENCY_WEIGHT: f32 = 8.0; // Adjacency weight when growing the city: much higher than ADJACENCY_WEIGHT so compactness dominates terrain similarity and the city stays clustered instead of stretching into tendrils
pub const URBAN_RELATIVE_TO_PRIME: f32 = 0.0; // score needed exceed to be under to be urban in relation to prime parcel

// Urban footprint regularization (see districts/footprint.rs).
pub const CLOSE_RADIUS: i32 = 4; // Morphological closing radius: fills concave bays/notches up to this size
pub const OPEN_RADIUS: i32 = 3; // Morphological opening radius: trims tendrils/peninsulas thinner than this
pub const FOOTPRINT_RECLASSIFY_THRESHOLD: f32 = 0.5; // Fraction of a district's cells inside the footprint to count it Urban
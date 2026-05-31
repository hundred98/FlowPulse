//! Arc geometry calculation module
//!
//! Provides arc interpolation for G2/G3 commands.

use std::f32::consts::PI;

const EPSILON: f32 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArcDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, Copy)]
pub struct ArcParams {
    pub start_x: f32,
    pub start_y: f32,
    pub end_x: f32,
    pub end_y: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
    pub direction: ArcDirection,
    pub full_turns: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct ArcSubdivision {
    pub dx: f32,
    pub dy: f32,
    pub dz: f32,
    pub de: f32,
    pub distance_xy: f32,
    pub fraction: f32,
}

impl ArcParams {
    pub fn from_ij(
        start_x: f32,
        start_y: f32,
        end_x: f32,
        end_y: f32,
        i_offset: f32,
        j_offset: f32,
        direction: ArcDirection,
        full_turns: u32,
    ) -> Option<Self> {
        let center_x = start_x + i_offset;
        let center_y = start_y + j_offset;
        let radius = (i_offset * i_offset + j_offset * j_offset).sqrt();
        
        if radius < EPSILON {
            return None;
        }
        
        Some(Self {
            start_x,
            start_y,
            end_x,
            end_y,
            center_x,
            center_y,
            radius,
            direction,
            full_turns,
        })
    }
    
    pub fn subdivide(&self, max_sag: f32) -> Vec<ArcSubdivision> {
        let chord_length = 2.0 * self.radius * (max_sag / self.radius).asin();
        let total_angle = self.calculate_total_angle();
        let num_segments = ((total_angle / chord_length).ceil() as usize).max(1);
        
        let mut subdivisions = Vec::with_capacity(num_segments);
        for i in 0..num_segments {
            let t = (i + 1) as f32 / num_segments as f32;
            let angle = self.start_angle() + t * total_angle;
            
            let x = self.center_x + self.radius * angle.cos();
            let y = self.center_y + self.radius * angle.sin();
            
            subdivisions.push(ArcSubdivision {
                dx: x - if i == 0 { self.start_x } else { self.center_x + self.radius * (self.start_angle() + (i as f32 / num_segments as f32) * total_angle).cos() },
                dy: y - if i == 0 { self.start_y } else { self.center_y + self.radius * (self.start_angle() + (i as f32 / num_segments as f32) * total_angle).sin() },
                dz: 0.0,
                de: 0.0,
                distance_xy: ((x - self.start_x).powi(2) + (y - self.start_y).powi(2)).sqrt(),
                fraction: t,
            });
        }
        
        subdivisions
    }
    
    fn start_angle(&self) -> f32 {
        (self.start_y - self.center_y).atan2(self.start_x - self.center_x)
    }
    
    fn end_angle(&self) -> f32 {
        (self.end_y - self.center_y).atan2(self.end_x - self.center_x)
    }
    
    fn calculate_total_angle(&self) -> f32 {
        let start = self.start_angle();
        let end = self.end_angle();
        let mut angle = end - start;
        
        if angle <= 0.0 && matches!(self.direction, ArcDirection::CounterClockwise) {
            angle += 2.0 * PI;
        } else if angle >= 0.0 && matches!(self.direction, ArcDirection::Clockwise) {
            angle -= 2.0 * PI;
        }
        
        angle += 2.0 * PI * self.full_turns as f32;
        angle
    }
}

pub struct ArcGeometry;

impl ArcGeometry {
    pub fn calculate_arc(params: &ArcParams, max_sag: f32) -> Vec<ArcSubdivision> {
        params.subdivide(max_sag)
    }
}

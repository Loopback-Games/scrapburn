//! Non-blocking GPU frame timing with timestamp queries.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const SLOTS: usize = 3;
const IDLE: u8 = 0;
const IN_FLIGHT: u8 = 1;
const MAPPED: u8 = 2;
const FAILED: u8 = 3;

struct Slot {
    queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    readback: wgpu::Buffer,
    state: Arc<AtomicU8>,
}

/// Times one render pass per frame with begin/end timestamps, read back a few frames later.
/// Never blocks: if every slot is still in flight, that frame simply isn't timed.
pub struct GpuTimer {
    slots: Vec<Slot>,
    period_ns: f32,
    current: Option<usize>,
    next: usize,
    last_ms: Option<f32>,
}

impl GpuTimer {
    /// `None` when the device lacks `TIMESTAMP_QUERY`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let slots = (0..SLOTS)
            .map(|_| Slot {
                queries: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("gpu timer"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                resolve: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gpu timer resolve"),
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("gpu timer readback"),
                    size: 16,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                state: Arc::new(AtomicU8::new(IDLE)),
            })
            .collect();
        Some(Self {
            slots,
            period_ns: queue.get_timestamp_period(),
            current: None,
            next: 0,
            last_ms: None,
        })
    }

    /// Beginning-of-frame timestamp for the first pass, or `None` if no slot is free.
    pub fn begin_writes(&mut self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        let i = self.next;
        if self.slots[i].state.load(Ordering::Acquire) != IDLE {
            self.current = None;
            return None;
        }
        self.current = Some(i);
        self.next = (i + 1) % SLOTS;
        Some(wgpu::RenderPassTimestampWrites {
            query_set: &self.slots[i].queries,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: None,
        })
    }

    /// End-of-frame timestamp for the last pass (same slot as `begin_writes`).
    pub fn end_writes(&self) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        self.current.map(|i| wgpu::RenderPassTimestampWrites {
            query_set: &self.slots[i].queries,
            beginning_of_pass_write_index: None,
            end_of_pass_write_index: Some(1),
        })
    }

    /// Records the query resolve and copy; call after the timed pass ends, before `finish`.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if let Some(i) = self.current {
            let s = &self.slots[i];
            encoder.resolve_query_set(&s.queries, 0..2, &s.resolve, 0);
            encoder.copy_buffer_to_buffer(&s.resolve, 0, &s.readback, 0, 16);
        }
    }

    /// Starts the readback; call right after `queue.submit`.
    pub fn after_submit(&mut self) {
        if let Some(i) = self.current.take() {
            let s = &self.slots[i];
            s.state.store(IN_FLIGHT, Ordering::Release);
            let state = s.state.clone();
            s.readback.map_async(wgpu::MapMode::Read, .., move |r| {
                state.store(if r.is_ok() { MAPPED } else { FAILED }, Ordering::Release);
            });
        }
    }

    /// Collects finished readbacks; returns the most recent GPU pass time in ms.
    pub fn poll_ms(&mut self, device: &wgpu::Device) -> Option<f32> {
        let _ = device.poll(wgpu::PollType::Poll);
        for s in &self.slots {
            match s.state.load(Ordering::Acquire) {
                MAPPED => {
                    if let Ok(view) = s.readback.get_mapped_range(..) {
                        let t: [u64; 2] = bytemuck::pod_read_unaligned(&view[..16]);
                        drop(view);
                        if t[1] > t[0] {
                            self.last_ms = Some((t[1] - t[0]) as f32 * self.period_ns / 1.0e6);
                        }
                    }
                    s.readback.unmap();
                    s.state.store(IDLE, Ordering::Release);
                }
                FAILED => s.state.store(IDLE, Ordering::Release),
                _ => {}
            }
        }
        self.last_ms
    }
}

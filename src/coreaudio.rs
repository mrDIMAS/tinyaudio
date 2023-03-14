//! macOS+iOS output device via `CoreAudio`

#![cfg(any(target_os = "macos", target_os = "ios"))]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use coreaudio_sys::*;
use std::{error::Error, ffi::c_void, mem::size_of};

type NativeSample = i16;

pub struct CoreaudioSoundDevice {
    // Keep send context alive while the device is alive.
    #[allow(dead_code)]
    inner: Box<SendContext>,
}

unsafe impl Send for CoreaudioSoundDevice {}

struct SendContext {
    data_callback: Box<dyn FnMut(&mut [f32]) + Send + 'static>,
    out_data: Vec<NativeSample>,
    mix_buffer: Vec<f32>,
    queue: AudioQueueRef,
    bufs: [AudioQueueBufferRef; 2],
}

impl Drop for SendContext {
    fn drop(&mut self) {
        unsafe {
            AudioQueueStop(self.queue, true as u8);
            // Dispose audio queue and all of its resources, including its buffers
            AudioQueueDispose(self.queue, false as u8);
        }
    }
}

fn check(error: OSStatus, msg: &str) -> Result<(), Box<dyn Error>> {
    if error == noErr as i32 {
        Ok(())
    } else {
        Err(format!("{}. Error code {}", msg, error).into())
    }
}

unsafe extern "C" fn audio_queue_callback(
    user_data: *mut c_void,
    queue: AudioQueueRef,
    buf: AudioQueueBufferRef,
) {
    let inner: &mut SendContext = &mut *(user_data as *mut SendContext);

    let buffer_len_bytes = inner.out_data.len() * size_of::<NativeSample>();

    (inner.data_callback)(&mut inner.mix_buffer);

    // Convert i16 -> f32
    debug_assert_eq!(inner.mix_buffer.len(), inner.out_data.len());
    for (in_sample, out_sample) in inner.mix_buffer.iter().zip(inner.out_data.iter_mut()) {
        *out_sample = (*in_sample * i16::MAX as f32) as i16;
    }

    // set the buffer data
    let src = inner.out_data.as_mut_ptr() as *mut u8;
    let dst = (*buf).mAudioData as *const u8 as *mut u8;
    std::ptr::copy_nonoverlapping(src, dst, buffer_len_bytes);

    AudioQueueEnqueueBuffer(queue, buf, 0, std::ptr::null_mut());
}

impl BaseAudioOutputDevice for CoreaudioSoundDevice {}

impl AudioOutputDevice for CoreaudioSoundDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
    {
        let buffer_len_bytes =
            params.channel_sample_count * params.channels_count * size_of::<NativeSample>();

        // 16-bit linear PCM
        let desc = AudioStreamBasicDescription {
            mSampleRate: params.sample_rate as f64,
            mFormatID: kAudioFormatLinearPCM,
            mFormatFlags: kLinearPCMFormatFlagIsSignedInteger | kLinearPCMFormatFlagIsPacked,
            mBitsPerChannel: 16,
            mFramesPerPacket: 1,
            mChannelsPerFrame: params.channels_count as u32,
            mBytesPerFrame: (params.channels_count * size_of::<NativeSample>()) as u32,
            mBytesPerPacket: (params.channels_count * size_of::<NativeSample>()) as u32,
            mReserved: 0,
        };

        // create data at fixed memory location
        let mut inner = Box::new(SendContext {
            data_callback: Box::new(data_callback),
            out_data: vec![0i16; params.channel_sample_count * params.channels_count],
            mix_buffer: vec![0.0; params.channel_sample_count * params.channels_count],
            queue: std::ptr::null_mut(),
            bufs: [std::ptr::null_mut(); 2],
        });

        inner.queue = {
            let mut queue = std::ptr::null_mut();
            let res = unsafe {
                AudioQueueNewOutput(
                    &desc,
                    Some(self::audio_queue_callback),
                    // `user_data` passed to ^ (`self::audio_queue_callback`)
                    (&mut *inner) as *const SendContext as *const c_void as *mut c_void,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    0,
                    &mut queue,
                )
            };

            self::check(res, "Failed to `AudioQueueNewOutput`")?;
            if queue == std::ptr::null_mut() {
                return Err("Succeeded in `AudioQueueNewOutput` but the queue is null".into());
            }

            queue
        };

        // create two audio buffers
        for i in 0..2 {
            inner.bufs[i] = {
                let mut buf: AudioQueueBufferRef = std::ptr::null_mut();
                let res = unsafe {
                    AudioQueueAllocateBuffer(inner.queue, buffer_len_bytes as u32, &mut buf)
                };

                check(res, "Failed to `AudioQueueAllocateBuffer`")?;
                if buf == std::ptr::null_mut() {
                    return Err(
                        "Succeeded in `AudioQueueAllocateBuffer` but the buffer is null"
                            .to_string()
                            .into(),
                    );
                }

                // fill the buffer with zeroes
                unsafe {
                    (*buf).mAudioDataByteSize = buffer_len_bytes as u32;

                    let data_ptr = (*buf).mAudioData;
                    std::ptr::write_bytes(
                        data_ptr as *const u8 as *mut u8,
                        0u8,
                        buffer_len_bytes as usize,
                    );

                    AudioQueueEnqueueBuffer(inner.queue, buf, 0, std::ptr::null_mut());
                }

                buf
            };
        }

        let res = unsafe { AudioQueueStart(inner.queue, std::ptr::null_mut()) };
        check(res, "Failed to `AudioQueueStart`")?;

        Ok(Self { inner })
    }
}

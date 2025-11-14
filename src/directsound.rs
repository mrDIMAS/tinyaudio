//! Windows output device via `DirectSound`.

#![cfg(target_os = "windows")]
#![allow(non_snake_case)]

use crate::{AudioOutputDevice, BaseAudioOutputDevice, OutputDeviceParameters};
use std::{
    error::Error,
    mem::size_of,
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
};
use winapi::{
    ctypes::c_void,
    shared::{
        guiddef::IID_NULL,
        minwindef::{DWORD, WORD},
        mmreg::{WAVEFORMATEX, WAVE_FORMAT_PCM},
        ntdef::{HANDLE, PVOID},
        winerror::HRESULT,
    },
    um::{
        dsound::*,
        synchapi::{CreateEventA, WaitForMultipleObjects},
        unknwnbase::{IUnknown, IUnknownVtbl},
        winbase::{INFINITE, WAIT_OBJECT_0},
        winuser::{GetDesktopWindow, GetForegroundWindow},
    },
    RIDL, STRUCT,
};

// Declare missing structs and interfaces.
#[allow(unexpected_cfgs)]
STRUCT! {struct DSBPOSITIONNOTIFY {
    dwOffset: DWORD,
    hEventNotify: HANDLE,
}}

RIDL! {#[uuid(0xb021_0783, 0x89cd, 0x11d0, 0xaf, 0x8, 0x0, 0xa0, 0xc9, 0x25, 0xcd, 0x16)]
interface IDirectSoundNotify(IDirectSoundNotifyVtbl): IUnknown(IUnknownVtbl) {
    fn SetNotificationPositions(
        dwPositionNotifies : DWORD,
        pcPositionNotifies : PVOID,
        ) -> HRESULT,
}}

const DSERR_BUFFERLOST: u32 = 0x88780096;
const DSERR_INVALIDCALL: u32 = 0x88780032;
const DSERR_INVALIDPARAM: u32 = 0x80070057;
const DSERR_PRIOLEVELNEEDED: u32 = 0x88780046;
const DSERR_ALLOCATED: u32 = 0x8878000A;
const DSERR_NODRIVER: u32 = 0x88780078;
const DSERR_OUTOFMEMORY: u32 = 0x00000007;
const DSERR_UNINITIALIZED: u32 = 0x887800AA;
const DSERR_UNSUPPORTED: u32 = 0x80004001;
const DSERR_CONTROLUNAVAIL: u32 = 0x8878001E;
const DSERR_BADFORMAT: u32 = 0x88780064;

type DeviceSample = i16;

pub struct DirectSoundDevice {
    direct_sound: *mut IDirectSound,
    data_sender_thread_handle: Option<JoinHandle<()>>,
    is_running: Arc<AtomicBool>,
}

fn check<S>(code: HRESULT, message: S) -> Result<(), Box<dyn Error>>
where
    S: AsRef<str>,
{
    // Handle only the error codes that may occur by the use of DirectSound methods used in this
    // module.
    let code_description = match code as u32 {
        0 => "No Error",
        DSERR_BUFFERLOST => "Buffer was lost.",
        DSERR_INVALIDCALL => "This function is not valid for the current state of this object.",
        DSERR_INVALIDPARAM => "An invalid parameter was passed to the returning function.",
        DSERR_PRIOLEVELNEEDED => "A cooperative level of DSSCL_PRIORITY or higher is required.",
        DSERR_ALLOCATED => {
            "The request failed because resources, such as a priority level, were \
        already in use by another caller."
        }
        DSERR_NODRIVER => {
            "No sound driver is available for use, or the given GUID is not a \
        valid DirectSound device ID."
        }
        DSERR_OUTOFMEMORY => {
            "The DirectSound subsystem could not allocate sufficient memory \
        to complete the caller's request."
        }
        DSERR_UNINITIALIZED => {
            "The IDirectSound8::Initialize method has not been called or has not \
            been called successfully before other methods were called."
        }
        DSERR_UNSUPPORTED => "The function called is not supported at this time.",
        DSERR_CONTROLUNAVAIL => {
            "The buffer control (volume, pan, and so on) requested by the caller is not \
            available. Controls must be specified when the buffer is created, using the \
            dwFlags member of DSBUFFERDESC."
        }
        DSERR_BADFORMAT => "The specified wave format is not supported.",
        _ => "Unknown",
    };

    if code == DS_OK {
        Ok(())
    } else {
        Err(format!("{}. Reason: {}", message.as_ref(), code_description).into())
    }
}

impl BaseAudioOutputDevice for DirectSoundDevice {}

unsafe impl Send for DirectSoundDevice {}

impl AudioOutputDevice for DirectSoundDevice {
    fn new<C>(params: OutputDeviceParameters, data_callback: C) -> Result<Self, Box<dyn Error>>
    where
        C: FnMut(&mut [f32]) + Send + 'static,
    {
        let OutputDeviceParameters {
            channels_count,
            channel_sample_count,
            sample_rate,
        } = params;

        let byte_per_sample = size_of::<DeviceSample>();
        let buffer_len_bytes = channels_count * byte_per_sample * channel_sample_count;
        let block_align = byte_per_sample * channels_count;

        let mut buffer_format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM,
            nChannels: channels_count as WORD,
            nSamplesPerSec: sample_rate as DWORD,
            nAvgBytesPerSec: (sample_rate * block_align) as DWORD,
            nBlockAlign: block_align as WORD,
            wBitsPerSample: (8 * byte_per_sample) as WORD,
            cbSize: size_of::<WAVEFORMATEX>() as WORD,
        };

        let buffer_desc = DSBUFFERDESC {
            dwSize: size_of::<DSBUFFERDESC>() as DWORD,
            dwFlags: DSBCAPS_CTRLPOSITIONNOTIFY | DSBCAPS_GLOBALFOCUS,
            // Buffer consists of two halves so we double the size here.
            dwBufferBytes: (2 * buffer_len_bytes) as DWORD,
            dwReserved: 0,
            lpwfxFormat: &mut buffer_format,
            guid3DAlgorithm: IID_NULL,
        };

        unsafe {
            let mut direct_sound = null_mut();
            check(
                DirectSoundCreate(null(), &mut direct_sound, null_mut()),
                "Failed to initialize DirectSound.",
            )?;

            let mut hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                hwnd = GetDesktopWindow();
            }

            check(
                (*direct_sound).SetCooperativeLevel(hwnd, DSSCL_PRIORITY),
                "Failed to set cooperative level.",
            )?;

            let mut buffer = null_mut();
            check(
                (*direct_sound).CreateSoundBuffer(&buffer_desc, &mut buffer, null_mut()),
                "Failed to create render buffer.",
            )?;

            let mut notify: *mut IDirectSoundNotify = null_mut();
            check(
                (*buffer).QueryInterface(
                    &IID_IDirectSoundNotify,
                    ((&mut notify) as *mut *mut _) as *mut *mut c_void,
                ),
                "Failed to obtain IDirectSoundNotify interface.",
            )?;

            let notify_points = [
                CreateEventA(null_mut(), 0, 0, null()),
                CreateEventA(null_mut(), 0, 0, null()),
            ];

            let mut pos = [
                DSBPOSITIONNOTIFY {
                    dwOffset: 0,
                    hEventNotify: notify_points[0],
                },
                DSBPOSITIONNOTIFY {
                    dwOffset: buffer_desc.dwBufferBytes / 2,
                    hEventNotify: notify_points[1],
                },
            ];

            check(
                (*notify).SetNotificationPositions(
                    pos.len() as DWORD,
                    &mut pos as *mut _ as *mut c_void,
                ),
                "Failed to set notification positions.",
            )?;

            check(
                (*buffer).Play(0, 0, DSBPLAY_LOOPING),
                "Failed to begin playing the render buffer.",
            )?;

            let is_running = Arc::new(AtomicBool::new(true));

            let data_sender_thread_handle = Some(
                DataSender {
                    buffer,
                    notify_points,
                    data_callback,
                    channels_count,
                    channel_sample_count,
                    is_running: is_running.clone(),
                }
                .run_in_thread(),
            );

            Ok(Self {
                direct_sound,
                data_sender_thread_handle,
                is_running,
            })
        }
    }
}

impl Drop for DirectSoundDevice {
    fn drop(&mut self) {
        unsafe {
            // Notify data sender thread that it should be stopped.
            self.is_running.store(false, Ordering::SeqCst);

            // Wait the thread to exit.
            self.data_sender_thread_handle
                .take()
                .expect("Malformed join handle!")
                .join()
                .expect("The thread must exist!");

            // Ensure that the ref counter is zero to the device is actually destroyed.
            assert_eq!((*self.direct_sound).Release(), 0);
        }
    }
}

struct DataSender<C> {
    buffer: *mut IDirectSoundBuffer,
    notify_points: [*mut c_void; 2],
    data_callback: C,
    channels_count: usize,
    channel_sample_count: usize,
    is_running: Arc<AtomicBool>,
}

unsafe impl<C> Send for DataSender<C> {}

impl<C> DataSender<C>
where
    C: FnMut(&mut [f32]) + Send + 'static,
{
    #[must_use]
    fn run_in_thread(mut self) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("DirectSoundFeedThread".to_string())
            .spawn(move || unsafe { self.run_send_loop() })
            .expect("Failed to create sender thread!")
    }

    unsafe fn run_send_loop(&mut self) {
        let mut data_buffer = vec![0.0; self.channel_sample_count * self.channels_count];
        let device_buffer_half_len_bytes = (data_buffer.len() * size_of::<DeviceSample>()) as DWORD;

        while self.is_running.load(Ordering::SeqCst) {
            (self.data_callback)(&mut data_buffer);

            // Wait and send.
            const WAIT_OBJECT_1: u32 = WAIT_OBJECT_0 + 1;
            match WaitForMultipleObjects(2, self.notify_points.as_ptr(), 0, INFINITE) {
                WAIT_OBJECT_0 => self.write(
                    device_buffer_half_len_bytes,
                    device_buffer_half_len_bytes,
                    &data_buffer,
                ),
                WAIT_OBJECT_1 => self.write(0, device_buffer_half_len_bytes, &data_buffer),
                _ => panic!("Unknown buffer point!"),
            }
        }
    }

    unsafe fn write(&self, offset_bytes: DWORD, len_bytes: DWORD, data_buffer: &[f32]) {
        let mut size = 0;
        let mut device_buffer = null_mut();
        check(
            (*self.buffer).Lock(
                offset_bytes,
                len_bytes,
                &mut device_buffer,
                &mut size,
                null_mut(),
                null_mut(),
                0,
            ),
            "Failed to lock the device buffer!",
        )
        .unwrap();

        let device_buffer_slice = std::slice::from_raw_parts_mut::<DeviceSample>(
            device_buffer as *mut _,
            data_buffer.len(),
        );

        debug_assert_eq!(size as usize, data_buffer.len() * size_of::<DeviceSample>());
        debug_assert_eq!(device_buffer_slice.len(), data_buffer.len());
        for (in_sample, out_sample) in data_buffer.iter().zip(device_buffer_slice) {
            *out_sample = (in_sample * DeviceSample::MAX as f32) as DeviceSample;
        }

        check(
            (*self.buffer).Unlock(device_buffer, size, null_mut(), 0),
            "Failed to unlock the device buffer!",
        )
        .unwrap();
    }
}

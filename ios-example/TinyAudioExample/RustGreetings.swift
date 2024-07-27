//
//  RustGreetings.swift
//  TinyAudioExample
//
//  Created by Dustin Bowers on 7/25/24.
//

import Foundation

class RustGreetings {
    init () {
        let status = create_audio_device();
        print("RustGreetings::init() - status = ", status)
    }
    
    func cleanup() {
        print("RustGreetings::cleanup()")
        if is_audio_initialized() > 0 {
            destroy_audio_device();
        }
    }
}

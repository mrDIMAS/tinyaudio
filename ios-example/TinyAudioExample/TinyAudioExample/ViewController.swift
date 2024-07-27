//
//  ViewController.swift
//  TinyAudioExample
//
//  Created by Dustin Bowers on 7/25/24.
//

import UIKit

class ViewController: UIViewController {
    
    var handle: RustGreetings?

    override func viewDidLoad() {
        super.viewDidLoad()
        // Do any additional setup after loading the view.
        
        if handle == nil {
            handle = RustGreetings()
        }
        
        // Add observers for app state changes
        NotificationCenter.default.addObserver(self, selector: #selector(handleAppWillResignActive), name: UIApplication.willResignActiveNotification, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(handleAppDidBecomeActive), name: UIApplication.didBecomeActiveNotification, object: nil)
    }

    @objc func handleAppWillResignActive() {
        // Clean up the audio device
        handle?.cleanup()
        handle = nil
    }

    @objc func handleAppDidBecomeActive() {
        // Re-initialize the audio device if needed
        if handle == nil {
            handle = RustGreetings()
        }
    }

    deinit {
        // Remove observers
        NotificationCenter.default.removeObserver(self, name: UIApplication.willResignActiveNotification, object: nil)
        NotificationCenter.default.removeObserver(self, name: UIApplication.didBecomeActiveNotification, object: nil)
        
        // Clean up the audio device
        handle?.cleanup()
        handle = nil
    }
}


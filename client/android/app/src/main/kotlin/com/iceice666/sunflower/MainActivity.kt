package com.iceice666.sunflower

import com.ryanheise.audioservice.AudioServiceActivity

// audio_service requires the main activity to extend AudioServiceActivity
// so it can connect to the foreground media service.
class MainActivity : AudioServiceActivity()

const play_sine_wave = import('./pkg/wasm_examples.js').then(({default: init, play_sine_wave}) =>
    init().then(() => play_sine_wave)
)
const elementTargetButton = document.querySelector('#button-start')
const elementMain = document.querySelector('#main')

const run = async () => {
    elementTargetButton.removeEventListener('click', run)
    elementMain.remove()
    const device = (await play_sine_wave)()

    // Play the sine wave for 5 seconds and then close the output device.
    setTimeout(() => {
        device.close()
    }, 5000)
}

elementTargetButton.addEventListener('click', run, {
    once: true,
    passive: true,
})

const play_sine_wave = import('./pkg/wasm_examples.js').then(({ default: init, play_sine_wave }) =>
  init().then(() => play_sine_wave)
)
const elementTargetButton = document.querySelector('#button-start')
const elementMain = document.querySelector('#main')

const run = async () => {
  elementTargetButton.removeEventListener('click', run)
  elementMain.remove()
  const f = await play_sine_wave;
  f()
}

elementTargetButton.addEventListener('click', run, {
  once: true,
  passive: true,
})

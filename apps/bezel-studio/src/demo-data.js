// Scenarios for demo mode (browser without Tauri): `?demo=<name>`.
const turing88 = Object.freeze({
  key: '/dev/ttyACM1',
  state: 'awake',
  family: 'turing-rev-c',
  models: [
    {
      id: 'turing-8.8',
      name: 'Turing Smart Screen 8.8"',
      diagonal: '8.8"',
      width: 480,
      height: 1920,
      capabilities: {
        brightness: true,
        deviceRotation: false,
        partialUpdate: true,
        backplateLed: false,
        storage: true,
        videoPlayback: true,
      },
      hardwareValidated: false,
    },
  ],
  display: { address: '/dev/ttyACM1', usb: '0525:a4a7', serial: null, manufacturer: null, product: null, location: '3-1.2' },
  wake: { address: '/dev/ttyACM0', usb: '1a86:ca88', serial: 'CT88INCH', manufacturer: 'Turing', product: 'UsbMonitor', location: '3-1.1' },
});

const asleep21 = Object.freeze({
  key: 'COM3',
  state: 'asleep',
  family: 'turing-rev-c',
  models: [
    { ...turing88.models[0], id: 'turing-2.1', name: 'Turing Smart Screen 2.1"', diagonal: '2.1"', width: 480, height: 480 },
    { ...turing88.models[0], id: 'turing-2.8', name: 'Turing Smart Screen 2.8"', diagonal: '2.8"', width: 480, height: 480 },
  ],
  display: null,
  wake: { address: 'COM3', usb: '1a86:ca21', serial: 'CT21INCH', manufacturer: 'Turing', product: 'UsbMonitor', location: null },
});

/** @type {Record<string, {screens?: object[], error?: string}>} */
export const SCENARIOS = Object.freeze({
  turing88: { screens: [turing88] },
  two: { screens: [turing88, asleep21] },
  empty: { screens: [] },
  error: { error: 'serial port enumeration: permission denied' },
});

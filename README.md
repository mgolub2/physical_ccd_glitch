# Physical CCD Glitch

A desktop app that emulates a real CCD image sensor and lets you manipulate its physics parameters to produce glitch art. Load any image, map it onto an emulated CCD, then twist the clocks, break the ADC, kill the anti-blooming gate, and watch your image fall apart in physically plausible ways.

Grounded in real CCD datasheets (KAF-6303, KAF-4320, ICX059CL, KAF-16803) and inspired by circuit bending, camera hacking, and pixel manipulation techniques.

## Installation

### macOS / Windows (prebuilt binaries)

Download the latest release from the [Releases page](https://github.com/mgolub2/physical_ccd_glitch/releases).

| Platform | Download |
|----------|----------|
| macOS (Apple Silicon) | `physical_ccd_glitch-macos-arm64.tar.gz` |
| macOS (Intel) | `physical_ccd_glitch-macos-x86_64.tar.gz` |
| Windows | `physical_ccd_glitch-windows-x86_64.zip` |

### Linux / from source

Requires a Rust toolchain (1.85+). On Linux you also need system dependencies for the GUI:

**Debian/Ubuntu:**
```bash
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libssl-dev libgtk-3-dev
```

**Fedora:**
```bash
sudo dnf install libxcb-devel libxkbcommon-devel openssl-devel gtk3-devel
```

**Arch:**
```bash
sudo pacman -S libxcb libxkbcommon openssl gtk3
```

Then install:
```bash
cargo install --git https://github.com/mgolub2/physical_ccd_glitch.git
```

Or clone and build manually:
```bash
git clone https://github.com/mgolub2/physical_ccd_glitch.git
cd physical_ccd_glitch
cargo run --release
```

## Usage

1. Launch the app
2. Click **Open Image** to load a source image (PNG, JPEG, TIFF, BMP, WebP)
3. Select a **sensor preset** (KAF-6303, KAF-4320, KAF-16803, ICX059CL, or Custom)
4. Expand parameter sections in the left panel and adjust sliders
5. Click **Process** to render, or enable **Auto** for live updates on parameter change
6. Click **Save Result** to export the glitched image

## CCD Emulation Pipeline

The image passes through a physically-modeled CCD pipeline in order:

| Stage | What it does |
|-------|-------------|
| **Sensor** | Maps RGB pixels to electron counts based on full well capacity |
| **Bayer CFA** | Applies a color filter array (RGGB, BGGR, GRBG, GBRG) |
| **Noise** | Dark current (Poisson), photon shot noise, read noise (Gaussian) |
| **Blooming** | Excess charge spills vertically/horizontally; anti-blooming gate controls drain |
| **V-Clock** | Parallel charge transfer with CTE loss, waveform distortion, row-level glitches |
| **H-Clock** | Serial readout with CTE trailing, ringing, pixel-level glitches, bidirectional mode |
| **Amplifier** | Gain, S-curve nonlinearity, kTC reset noise, amplifier glow gradient |
| **ADC** | Bit depth quantization, correlated double sampling, DNL errors, random bit flips, jitter |
| **Glitch FX** | Pixel/block shift, scan line corruption, bit-plane XOR/rotation/swap |
| **Demosaic** | Reconstruct RGB from Bayer mosaic (bilinear or Malvar-He-Cutler) |
| **Color** | White balance, sRGB gamma, brightness/contrast, channel gain/swap, chromatic aberration |

## Sensor Presets

| Preset | Resolution | Pixel Size | Full Well (ABG / no-ABG) | Type |
|--------|-----------|------------|--------------------------|------|
| KAF-6303 | 3072 x 2048 | 9 x 9 um | 40k / 100k e- | Full-frame |
| KAF-4320 | 2048 x 2048 | 24 x 24 um | 90k / 150k e- | Full-frame |
| KAF-16803 | 4096 x 4096 | 9 x 9 um | 60k / 100k e- | Full-frame |
| ICX059CL | 500 x 582 | 9.8 x 6.3 um | 30k e- | Interline |

## License

MIT

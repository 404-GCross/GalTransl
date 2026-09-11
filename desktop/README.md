# GalTransl Desktop

This folder contains the Tauri desktop shell and the React frontend for GalTransl.

## Development

1. Install frontend dependencies:
   `npm ci`
2. Start the Python backend from the repository root:
   `python run_backend.py --host 127.0.0.1 --port 12333`
3. Start the frontend dev server:
   `npm run dev`
4. With Rust installed, run the desktop shell:
   `npm run tauri:dev`

On Linux x86_64, `../run_desktop_dev.sh` performs steps 2-4 and stops the
backend when the desktop process exits. The frontend remains browser-compatible
and talks to the Python backend over HTTP.

## Linux build

Install the Tauri Linux prerequisites for your distribution, then run:

```bash
python ../build_linux_x64.py
```

The script builds the PyInstaller backend, embeds it as a Tauri sidecar, bundles
`plugins`, `Dict`, `translation_guidelines`, and `res`, and generates `.deb`,
`.rpm`, `.AppImage`, and portable `.tar.gz` artifacts. Only x86_64 is supported;
ARM targets are intentionally out of scope.

# GalTransl Linux x86_64 builds

This branch is generated automatically by the **Linux x86_64** GitHub Actions workflow.

- Source commit: `2fbb0d7e6548d28f30b2b7ef25aadd2efacc6546`
- Workflow run: `5`
- Supported architecture: `x86_64`
- Package formats: `.deb`, `.rpm`, `.AppImage.xz`, `.tar.gz`

For a single AppImage.xz file, run:

```bash
xz -dk GalTransl_*_linux_x86_64.AppImage.xz
chmod +x GalTransl_*_linux_x86_64.AppImage
./GalTransl_*_linux_x86_64.AppImage
```

If split `.part-*` files are present, merge them first:

```bash
cat GalTransl_*_linux_x86_64.AppImage.xz.part-* > GalTransl_7.4.0_linux_x86_64.AppImage.xz
xz -dk GalTransl_7.4.0_linux_x86_64.AppImage.xz
chmod +x GalTransl_7.4.0_linux_x86_64.AppImage
./GalTransl_7.4.0_linux_x86_64.AppImage
```

Do not edit files on this branch manually. New builds replace the previous contents.

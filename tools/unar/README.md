# The Unarchiver's tools

Zipling reads classic Mac formats (StuffIt, BinHex, MacBinary, Compact Pro, DiskDoubler), the long tail of old formats 7-Zip doesn't (ARC, Zoo, PackIt, LBR…), and archives whose file names were written in a legacy code page (Shift-JIS, GB18030, Big5, EUC-KR, CP437…) with The Unarchiver's command-line tools, `lsar` and `unar`.

`build.sh` downloads MacPaw's [XADMaster](https://github.com/MacPaw/XADMaster) v1.10.8 and [UniversalDetector](https://github.com/MacPaw/universal-detector) 1.1, checks their SHA-256, and builds both tools for the app's deployment target into `tools/unar/bin/`. Xcode copies them into `Zipling.app/Contents/Helpers/` and signs them. Run it once:

```bash
tools/unar/build.sh
```

Until it has run, debug builds use Homebrew's copies (`brew install unar`). Release builds require the bundled ones.

## Licence

XADMaster and UniversalDetector are under the GNU Lesser General Public License, version 2.1 or later. Zipling runs the tools unmodified, as separate programs, and doesn't link them. The app's Legal page credits them and links to this exact source; anyone can rebuild and replace the tools inside the app bundle.

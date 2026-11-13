# Signal Processing Toolbox: coverage plan

How MatLivre grows to cover the MATLAB Signal Processing Toolbox. The function inventory is
`tools/coverage/signal.tsv` (category, name), taken from the toolbox's function reference
(mathworks.com/help/signal/referencelist.html, R2026b listing). Functions the reference marks
"Since R2024a" or later are left out, because MatLivre targets R2023b.

## Where things stand

| | Count |
|---|---:|
| Functions in scope (R2023b-era, from `signal.tsv`) | 333 |
| Already in MatLivre before this plan (`xcorr`, `xcov`, `resample`, `rms`, `sinc`, `detrend`, `movmedian`, `movmad`, `isoutlier`, `filloutliers`, `smoothdata`, `islocalmax`, `paddata`, `trimdata`, `resize`, `freqspace`, `ss2tf`) | 17 |
| Implemented by this plan (`crates/livre-lang/src/signal/`) | 199 |
| Still to implement | 117 |

The 199 are checked against MATLAB: see "How it is tested".

## How it is tested

`crates/livre-lang/tests/all/signal_matlab.rs` runs `data/signal_cases.m` (902 calls covering valid
use, option variants, orientation, edge cases and error messages) and compares it with
`data/signal_matlab.txt`, recorded from MATLAB R2023b Update 3 with Signal Processing Toolbox 23.2.
Classes, sizes and error messages must match exactly and numbers to a relative 1e-9 (1e-6 for single
precision). 881 of the 902 cases match; the other 21 are listed in
`data/signal_known_deviations.tsv` with the reason, and the test fails if one of them starts to
match, so the list only shrinks.

To add a function: add its calls to `tools/signal/cases.txt` (each line assigns `r`, a number array
or a cell array of them), run `python tools/signal/gen_cases.py` to rebuild `signal_cases.m`, run
that in MATLAB (`matlab -batch "run('signal_cases.m')"`) and keep its result lines as
`signal_matlab.txt` (the generator's docstring has the commands). The same script runs here; any
difference is either a bug to fix or an entry for the known-deviations list.

Running MATLAB over the 76 functions found most of the conventions the reference leaves open
(window length 0 and 1, row or column orientation, zero padding of transformed filters, the order
of `chirp`'s quadratic shapes, automatic lengths for `impz` and `freqs`, the exact wording of
validation messages) and several outright mistakes in the first, documentation-only
implementation. Those are now fixed and covered by the recorded cases.

## Out of scope

Following the existing scope rules for a live-script engine:

- Apps and interactive tools (Signal Analyzer, Signal Labeler, Signal Feature Extractor, Filter
  Designer, Filter Analyzer, Window Designer, WVTool, Wavelet and Multiresolution analyzers,
  EDF File Analyzer) and the Live Editor tasks (Design Filter, Create Plot).
- Deep-learning layers and datastores: `signalDatastore`, `cwtLayer`, `stftLayer`,
  `modwtLayer`, `dlcwt`, `dlstft`, `dlmodwt`, `deepSignalAnomalyDetector`.
- `filt2block` (Simulink) and code generation.
- The Slepian database functions `dpssclear`, `dpssdir`, `dpssload`, `dpsssave`
  (`dpss` itself is in scope).
- Signal labeling objects and region-of-interest functions (15 functions, object-heavy).
- EDF file I/O (`edfheader`, `edfinfo`, `edfread`, `edfwrite`): a file-format task.

That removes about 32 of the 333 names from the practical target; they stay in `signal.tsv`
so the coverage page can list them as out of scope.

## Phases

Each phase is a set of modules under `crates/livre-lang/src/signal/`, wired like the other
builtin modules (a `NAMES` list per module, `signal::call` as the router). New functions get
cases in `signal_cases.m` and MATLAB's output recorded as above.

1. **Windows, decibel conversions, basic waveforms. Done**: `rectwin`, `bartlett`, `triang`,
   `hamming`, `hann`, `blackman`, `blackmanharris`, `nuttallwin`, `flattopwin`, `kaiser`,
   `tukeywin`, `gausswin`, `parzenwin`, `bohmanwin`, `barthannwin`, `chebwin`; `mag2db`, `db2mag`,
   `pow2db`, `db2pow`; `square`, `sawtooth`, `rectpuls`, `diric`, `chirp`, `gmonopuls`.
2. **More waveforms and windows**. Done: `tripuls`, `gauspuls`, `buffer`, `shiftdata`, `unshiftdata`, `uencode`, `udecode`, `taylorwin`,
   `pulstran`, `vco`, `modulate` (`pwm` and `ppm` are not implemented). To do: `dpss`, `demod`, `marcumq`.
3. **Polynomial and filter conversions**. Done: `lp2lp`, `lp2hp`, `lp2bp`, `lp2bs` (transfer
   function form), `bilinear` (transfer function and zero-pole-gain forms, with pre-warping),
   `polystab`, `polyscale`, `eqtflength`, `sos2tf`, `tf2zp`, `tf2zpk`, `zp2tf`, `sos2zp`, `zp2sos`, `tf2sos`, `tf2ss`, `zp2ss`, `ss2zp`, `sos2ss`, `ss2sos`, `residuez`, `latc2tf`, `tf2latc`, `impinvar`, `cell2sos`, `sos2cell`.
4. **Analog prototypes and IIR design**. Done: `buttap`, `cheb1ap`, `cheb2ap`, `butter`,
   `cheby1`, `cheby2`, `ellipap`, `besselap`, `ellip`, `besself`, `buttord`, `cheb1ord`, `cheb2ord`, `ellipord` (digital and analog, all four band
   types, transfer function, zero-pole-gain and state-space outputs). To do: `yulewalk`,
   `maxflat`.
5. **FIR design and filter analysis**. Done: `fir1`, `freqz`, `freqs`, `grpdelay`, `phasez`, `phasedelay`, `zerophase`,
   `isstable`, `impz`, `stepz`, `sgolay`, `fir2`, `firls`, `kaiserord`, `firpm`, `firpmord`, `gaussdesign`, `rcosdesign`, `intfilt`, `impzlength`, `filternorm`, `filtord`,
   `firtype`, `isminphase`, `ismaxphase`, `islinphase`, `isallpass`. To do: `cfirpm`,
   `fircls`, `fircls1`; `zplane`, `designfilt`, `digitalFilter`.
6. **Filtering and multirate**. Done: `decimate`, `interp`, `latcfilt`, `filtfilt` (transfer function form), `sosfilt`, `filtic`,
   `medfilt1`, `downsample`, `upsample`, `fftfilt`, `upfirdn`, `sgolayfilt`, `hampel`. To do:
   `lowpass`, `highpass`, `bandpass`, `bandstop`, `fillgaps`;
   `filtfilt` with second-order sections.
7. **Spectral estimation and transforms**. Done: `periodogram`, `pwelch`, `mscohere`, `cpsd`, `tfestimate`, `dct`,
   `idct`, `dftmtx`, `goertzel`, `hilbert`, `cconv`, `convmtx`, `bitrevorder`, `czt`, `fwht`, `ifwht`, `cceps`, `rceps`, `icceps`,
   `digitrevorder`, `corrmtx`, `pyulear`, `pburg`, `pcov`, `pmcov`, `xcorr2`. To do: `pmtm`, `plomb`, `poctave`, `pspectrum`,
   `pmusic`, `peig`, `rootmusic`, `rooteig`.
8. **Measurements and modeling**. Done: `peak2peak`, `peak2rms`, `rssq`, `findpeaks`, `enbw`, `bandpower`, `meanfreq`, `medfreq`,
   `finddelay`, `seqperiod`, `cusum`, `dtw`, `edr`, `alignsignals`, `obw`, `powerbw`, `statelevels`, `midcross`, `risetime`, `falltime`,
   `pulsewidth`, `pulseperiod`, `pulsesep`, `dutycycle`, `overshoot`, `undershoot`, `settlingtime`, `slewrate`, `zerocrossrate`, `findchangepts`, `envelope`, `invfreqz`, `invfreqs` (non-iterative fit), `stmcb`. To do:
   `snr`, `sinad`, `thd`, `sfdr`, `toi`,
   `findsignal`;
   Done: `levinson`, `ac2rc`, `rc2ac`, `rc2poly`, `poly2rc`, `ac2poly`, `poly2ac`, `rc2is`, `is2rc`,
   `rc2lar`, `lar2rc`, `poly2lsf`, `lsf2poly`, `aryule`, `arburg`, `arcov`, `armcov`,
   `rlevinson`, `schurrc`, `prony`, `lpc`.
9. **Time-frequency**. Done: `spectrogram`. To do: `stft`, `istft`, `xspectrogram`, `iscola`,
   `wvd`, `xwvd`, `hht`, `emd`, `vmd`, `ewt`, `fsst`, `ifsst`, `tfridge`, `kurtogram`, `instfreq`,
   `instbw`, spectral descriptors (`spectralCrest`, `spectralEntropy`, `spectralFlatness`,
   `spectralKurtosis`, `spectralSkewness`), then the wavelet functions (`cwt`, `modwt`,
   `modwpt`, `wsst`, `wcoherence`, `tqwt`, `cqt`).
10. **Vibration analysis**: `rpmtrack`, `tachorpm`, `orderspectrum`, `ordertrack`,
    `orderwaveform`, `rpmordermap`, `rpmfreqmap`, `envspectrum`, `tsa`, `rainflow`, `modalfrf`,
    `modalfit`, `modalsd`.

## Known deviations from MATLAB

These are the calls in `signal_known_deviations.tsv`, plus the limits that the recorded cases do
not exercise:

- **Order of zeros and poles** of `cheby2` designs. `butter`, `cheby1` and the poles of `cheby2`
  follow MATLAB's order, because the design goes through the same state-space path (cascade
  realisation `zp2ss`, frequency transformation, bilinear transform) and takes the eigenvalue
  order of the state matrix from a port of LAPACK's `dgebal`/`dgehd2`/`dlahqr`
  (`signal/lapack_eig.rs`). The zeros of `cheby2` are the eigenvalues of A - B C / D; in about one
  case in twenty MATLAB's order follows rounding errors in the entries of that matrix, which
  differ here (five recorded cases).
- **Degenerate designs**: `cheby1` with zero ripple and `cheby2` with zero attenuation.
- **Single precision**: results are computed in double precision and rounded to single; MATLAB's
  own single arithmetic can differ in the last digits (the recorded cases allow 1e-6).
- **N-D arrays** and operating along dimension 3 (`dct`, `hilbert`).
- **`spectrogram`**: the centroid outputs `fc`, `tc`, and a frequency vector in place of `nfft`.
- **`pwelch`** with a non-integer overlap; **`freqz`** with a zero denominator;
  **`filtic`** with a zero leading denominator coefficient; **`gauspuls`** with complex time.
- **`grpdelay`** near zeros on the unit circle: MATLAB's own value loses accuracy there (3e-7
  off at the last point of a 5th-order Butterworth); MatLivre's is within 1e-8 of the exact one.
- **`dct`/`idct`** sum the definition directly, O(N^2) per column; fine for a few thousand samples,
  slow beyond that.
- **`fftfilt`** and **`upfirdn`** filter directly instead of with overlap-add FFTs or a polyphase
  structure; same results up to rounding.
- **`filtfilt`** takes a transfer function only (no second-order sections).
- **`impz`/`stepz`** choose their automatic length from the poles with the rule inferred from
  MATLAB's output (floor of ln(5e-5)/ln(radius) for stable poles, ln(1e6)/ln(radius) for unstable
  ones, five periods on the unit circle); unusual mixtures of pole radii may differ by a sample.
- **`freqs`** chooses its automatic range from the root magnitudes (a root at the origin counts
  as 1) by a rule inferred from MATLAB's output. MATLAB also refines the grid around resonances
  (second-order or higher denominators), which MatLivre does not: it spaces the points evenly.
- **`hilbert`** ignores the imaginary part of complex input, with MATLAB's warning.
- **Messages**: the wording follows MATLAB's for the cases recorded; other validation failures
  fall back to generic text.

## Per-phase checklist

From `AGENTS.md`:

1. Implement in `src/signal/<area>.rs`; add the area's `NAMES` to the combined list in
   `builtins.rs` and to the router in `signal/mod.rs`.
2. Add calls to `tests/all/data/signal_cases.m`, record MATLAB's output into
   `signal_matlab.txt`, and fix or list every difference.
3. Build and run tests one at a time (shared machine; no other `cargo` running).
4. Commit the implementation, run `bash tools/coverage/update.sh`, commit "Refresh the
   coverage checklist".
5. Update `README.md`: function count and the feature lists.

## Tooling still to do

- Feed `tools/coverage/signal.tsv` into `update.sh` / `coverage.pl` so
  `docs/matlab-coverage.md` gains a "Signal Processing Toolbox" section. `documented.tsv` is
  generated from the installation's `helpfuncbycat.xml`; the toolbox's reference list is now
  available in the MATLAB installation, so it can be generated the same way.
- A test that every name in each module's `NAMES` is dispatched is covered by the recorded cases
  for the implemented functions; new functions need their cases.

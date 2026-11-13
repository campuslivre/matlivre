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
| Implemented by this plan (`crates/livre-lang/src/signal/`) | 221 |
| Still to implement and verifiable here | 81 |
| Not verifiable: requires Wavelet/Audio Toolbox, not installed (see below) | 14 |

The 221 are checked against MATLAB: see "How it is tested".

## Not verifiable: requires Wavelet/Audio Toolbox, not installed

The MATLAB install used for recording has neither the Wavelet nor the Audio Toolbox, so these cannot be
recorded and are not implemented: `cwt`, `modwt`, `modwpt`, `imodwt`, `wsst`, `wcoherence`, `tqwt`, `cqt`,
`ewt`, `spectralCrest`, `spectralEntropy`, `spectralFlatness`, `spectralKurtosis`, `spectralSkewness`.
They stay in the gap count; the `dwt`/`wavedec` family is likewise unrecordable.

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
   `pulstran`, `vco`, `modulate` (`pwm` and `ppm` are not implemented), `dpss` (direct method only), `marcumq`, `demod` (`pwm` and `ppm` are not implemented).
3. **Polynomial and filter conversions**. Done: `lp2lp`, `lp2hp`, `lp2bp`, `lp2bs` (transfer
   function form), `bilinear` (transfer function and zero-pole-gain forms, with pre-warping),
   `polystab`, `polyscale`, `eqtflength`, `sos2tf`, `tf2zp`, `tf2zpk`, `zp2tf`, `sos2zp`, `zp2sos`, `tf2sos`, `tf2ss`, `zp2ss`, `ss2zp`, `sos2ss`, `ss2sos`, `residuez`, `latc2tf`, `tf2latc`, `impinvar`, `cell2sos`, `sos2cell`.
4. **Analog prototypes and IIR design**. Done: `buttap`, `cheb1ap`, `cheb2ap`, `butter`,
   `cheby1`, `cheby2`, `ellipap`, `besselap`, `ellip`, `besself`, `buttord`, `cheb1ord`, `cheb2ord`, `ellipord` (digital and analog, all four band
   types, transfer function, zero-pole-gain and state-space outputs), `maxflat` (without the second-order-section
   outputs). To do: `yulewalk`.
5. **FIR design and filter analysis**. Done: `fir1`, `freqz`, `freqs`, `grpdelay`, `phasez`, `phasedelay`, `zerophase`,
   `isstable`, `impz`, `stepz`, `sgolay`, `fir2`, `firls`, `kaiserord`, `firpm`, `firpmord`, `gaussdesign`, `rcosdesign`, `intfilt`, `impzlength`, `filternorm`, `filtord`,
   `firtype`, `isminphase`, `ismaxphase`, `islinphase`, `isallpass`, `fircls`, `fircls1` (without the WT and
   WP/WS/K options), `zplane` (zeros, poles, multiplicity labels and axes of the plot; not the `digitalFilter` input).
   `cfirpm` (amplitude vectors and your own response functions; not the predefined `@lowpass` and the other built-in responses).
   To do: `designfilt`, `digitalFilter`.
6. **Filtering and multirate**. Done: `decimate`, `interp`, `latcfilt`, `filtfilt` (transfer function form), `sosfilt`, `filtic`,
   `medfilt1`, `downsample`, `upsample`, `fftfilt`, `upfirdn`, `sgolayfilt`, `hampel`. To do:
   `lowpass`, `highpass`, `bandpass`, `bandstop`, `fillgaps`;
   `filtfilt` with second-order sections.
7. **Spectral estimation and transforms**. Done: `periodogram`, `pwelch`, `mscohere`, `cpsd`, `tfestimate`, `dct`,
   `idct`, `dftmtx`, `goertzel`, `hilbert`, `cconv`, `convmtx`, `bitrevorder`, `czt`, `fwht`, `ifwht`, `cceps`, `rceps`, `icceps`,
   `digitrevorder`, `corrmtx`, `pyulear`, `pburg`, `pcov`, `pmcov`, `xcorr2`, `pmusic`, `peig`, `rootmusic`, `rooteig`, `pmtm`, `plomb`, `poctave` (power and PSD smoothing; not `Weighting` or the spectrogram type).
   Not implemented after one dense round: `pspectrum` (rule not recovered, see the research queue).
8. **Measurements and modeling**. Done: `peak2peak`, `peak2rms`, `rssq`, `findpeaks`, `enbw`, `bandpower`, `meanfreq`, `medfreq`,
   `finddelay`, `seqperiod`, `cusum`, `dtw`, `edr`, `alignsignals`, `obw`, `powerbw`, `statelevels`, `midcross`, `risetime`, `falltime`,
   `pulsewidth`, `pulseperiod`, `pulsesep`, `dutycycle`, `overshoot`, `undershoot`, `settlingtime`, `slewrate`, `zerocrossrate`, `findchangepts`, `envelope`, `invfreqz`, `invfreqs` (non-iterative fit), `stmcb`, `findsignal` (fixed time alignment only). To do:
   `snr`, `sinad`, `thd`, `sfdr`, `toi`;
   Done: `levinson`, `ac2rc`, `rc2ac`, `rc2poly`, `poly2rc`, `ac2poly`, `poly2ac`, `rc2is`, `is2rc`,
   `rc2lar`, `lar2rc`, `poly2lsf`, `lsf2poly`, `aryule`, `arburg`, `arcov`, `armcov`,
   `rlevinson`, `schurrc`, `prony`, `lpc`.
9. **Time-frequency**. Done: `spectrogram`, `iscola`, `stft`, `istft`, `wvd` (not `smoothedPseudo`), `instfreq` (the 'hilbert' method only; the default 'tfmoment' method needs `pspectrum`'s spectrogram). Not recovered after one round: `xwvd`. To do: `xspectrogram`,
   `hht`, `emd`, `vmd`, `ewt`, `fsst`, `ifsst`, `tfridge`, `kurtogram`, `instfreq`,
   `instbw`, spectral descriptors (`spectralCrest`, `spectralEntropy`, `spectralFlatness`,
   `spectralKurtosis`, `spectralSkewness`) and the wavelet functions (`cwt`, `modwt`,
   `modwpt`, `wsst`, `wcoherence`, `tqwt`, `cqt`): the last two groups are not verifiable here.
10. **Vibration analysis**. Done: `rainflow`. To do: `rpmtrack`, `tachorpm`, `orderspectrum`, `ordertrack`,
    `orderwaveform`, `rpmordermap`, `rpmfreqmap`, `envspectrum`, `tsa`, `modalfrf`,
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
- **`fircls`/`fircls1`**: MatLivre returns the exact constrained optimum; MATLAB stops when the bounds
  are violated by under about 5e-9, so its coefficients differ by 1e-7 to 2e-6 (every numeric
  recorded call is a listed deviation for this reason). The WT and WP/WS/K options of `fircls1` and
  zero lower bounds in `fircls` are not implemented.
  One extra round tried a stopping tolerance of 5e-9 on the exact active-set iteration (the exchange
  iteration of the 1996 paper is not reproducible step for step from the published description);
  no recorded call moved closer, so the exact optimum stays.
- **`pmusic`/`peig`**: the noise eigenvectors (third output) agree with MATLAB up to the sign of each
  column (or the basis of a nearly degenerate noise subspace); the pseudospectrum, frequencies and
  eigenvalues agree. MATLAB's data matrix holds each segment in reverse order, and for an explicit
  window length the last segment is zero padded and the eigenvalues are normalised by one less than
  the segment count; `peig` weights use a different normalisation again (see `subspace.rs`).
- **Deferred queue** (after the research items): `cfirpm` predefined responses (`@lowpass`, `@multiband`, ...); `line(x,y,z)` does not store `ZData`; `line([],[])` returns a 0x0 placeholder instead of a 0x1 `Line`.
- **Test time**: the cfirpm designs of order 16 and above (about 9 s in a debug build) run only in the
  ignored test `signal_heavy_cases_match_matlab` (`cargo test -p livre-lang --test all -- --ignored`);
  the regular `signal_functions_match_matlab` skips them.
- **Research queue**: `xwvd` and `wvd(...,'smoothedPseudo')` (plain `wvd`: products x(a) y*(b) with a+b = 2t and lags |m| <= N/2 at N frequencies k/(2N) cycles per sample, real input made analytic from the record zero padded to 2N; the cross distribution does not follow that rule); `pspectrum` (power type only first): the output is always 4096 points from 0 to Nyquist, equal to twice the Kaiser (beta 40(1-leakage)) windowed modified periodogram of the whole record for short signals, but it deviates from the exact DTFT of that window by up to 1e-4 between some frequencies (an interpolation from a coarser grid?) and the Welch regime for long records (more than about 20000 samples) was not probed; `poctave` low-band filters (one round: decimate-then-filter and an independent bilinear design did not match); `rootmusic`/`rooteig` powers (systematic ~1e-7 relative; one round tried: mean noise eigenvalue as the noise variance, p/2 or p signal eigenvalues, exact or least squares; the zero-frequency root case gave P=[-0.24, 44.6] with a 2x2 solve, not MATLAB's [0, 34.29]).
- **`rootmusic`/`rooteig`**: frequencies agree to 1e-12, powers to about 1e-7 relative (MATLAB's
  power system was not recovered); a real correlation matrix with a root at zero frequency returns
  one power where MATLAB returns two.
- **`poctave`**: the ANSI S1.11 center frequencies; bands that overlap the limits (the band holding
  Nyquist is kept while its lower edge is under 0.9 of the upper limit and is a highpass of order
  FilterOrder); each band is a Butterworth bandpass of order FilterOrder (the SOS form) and the
  power is the mean square of its output. Bands with a center frequency below about 1/250 of the
  sample rate do not match (see the research queue), and `Weighting` and the spectrogram type are
  not implemented. PSD smoothing (rectangle rule, edge bins at 0 and Nyquist counted twice) matches.
- **`plomb`**: MATLAB uses the Press-Rybicki fast algorithm (4-point extirpolation, a power-of-two
  FFT, period ofac*N*ts), so MatLivre does too; a given frequency vector is evaluated exactly. The
  spectrum is scaled by 2 var ts (psd; ts from the whole record), 2 var / N (power) or 1; the
  detection thresholds use N*fmax/fNyquist independent frequencies. Differences: single precision and
  two-to-five-sample records (rounding).
- **`pmtm`**: the adaptive combination (Thomson weights from an initial two-taper average) stops
  when the summed change of the spectrum over the FFT grid falls below 5e-4 times the signal power
  (the rule that fits every recorded call); the confidence bounds use 2k degrees of freedom for all
  three combinations; with `ConfidenceLevel` the outputs come as `[pxx,w,pxxc]`. Not matched: nothing
  recorded; sine tapers, matrix input, complex input and custom frequencies are covered.
- **`cfirpm`**: MATLAB's result is the exact Chebyshev optimum on its frequency grid (the band edges
  plus the multiples of 2/L at least half a step from both edges; L = 2^nextpow2(0.8 lgrid (n+1))
  for the default lgrid of 25, which other lgrid values only approximate; for SYM='none' bands from -1
  to 1 the last two points become 1 - 1/(lgrid (n+1)); `real` drops the point one step below 1).
  MatLivre finds that optimum with a barrier method. Listed deviations: positive-frequency-only
  complex designs (the optimum is not unique), results MATLAB stops short of the optimum by about
  1e-6, negative band weights, complex amplitude values, and the `res` extremal lists. The
  predefined responses (`@lowpass`, `@bandpass`, ...) raise a "not implemented" error. Odd-symmetric
  designs from amplitude vectors return a zero filter, as MATLAB does.
- **`zplane`**: the unit circle has 100 points, more as a root nears the circle (round(1/distance),
  at most 50000); the axis limits are the 10 percent padded extents of everything drawn, widened to
  the axes box ratio. Not matched: single precision data (MatLivre's line objects store doubles), the
  limits left by a second call with hold on, and axes made by subplot (their Position is not tracked).
  `zplane(d)` for a `digitalFilter` is not implemented.
- **`maxflat`**: the second-order-section outputs (`sos`, `g`) are not implemented (MATLAB's section
  order and pairing were not reproduced); orders around 30 and the degenerate Wn = 0.5 designs
  differ at 1e-6 to 1e-8 because the flatness system is ill-conditioned.
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

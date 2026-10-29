# MATLAB coverage

Every public function of MATLAB R2023b in scope for matlivre, from the `Contents.m` files and the
public function files and classes of these folders of the installation, checked against matlivre.
Scope: the core MATLAB language, math, data analysis, strings, data types, dates, file I/O and
graphics (`toolbox/matlab/...`), and the Symbolic Math Toolbox (`toolbox/symbolic`).

Generated on 2026-10-29 from `master`. Tick items as they land.

| Status | Count |
|---|---:|
| Implemented | 487 |
| In a branch being finished (polyfun, matfun, datafun) | 152 |
| Language keyword (check it works) | 31 |
| **To implement** | **815** |
| Out of scope | 417 |

Out of scope, with the reason given for each: Java/.NET/COM/C interop and MEX, desktop/IDE/GUI and
printing tools, audio/video devices, scientific file formats (HDF5, netCDF, CDF, FITS, TIFF objects)
and web services, map tiles, internal helpers, and MATLAB example files.

## matlab/audiovideo

Implemented: 0. To implement: 5. In a branch: 0. Out of scope: 14.

To implement:

- [ ] `audioinfo`
- [ ] `audioread`
- [ ] `audiowrite`
- [ ] `lin2mu`
- [ ] `mu2lin`

Out of scope (audio/video devices or screen capture): `VideoReader`, `VideoWriter`, `audiodevinfo`, `audiodevreset`, `audioplayer`, `audiorecorder`, `audiouniquename`, `avgate`, `avifinfo`, `aviinfo`, `mmfileinfo`, `prefspanel`, `sound`, `soundsc`

## matlab/datafun

Implemented: 23. To implement: 2. In a branch: 70. Out of scope: 0.

To implement:

- [ ] `kde`
- [ ] `rms`

In a branch: `bounds`, `conv`, `conv2`, `convn`, `corrcoef`, `cov`, `deconv`, `del2`, `detrend`, `discretize`, `fft`, `fft2`, `fftn`, `fftshift`, `fftw`, `fillmissing`, `fillmissing2`, `filloutliers`, `filter`, `filter2`, `findgroups`, `gradient`, `groupcounts`, `groupfilter`, `groupsummary`, `grouptransform`, `histc`, `histcounts2`, `ifft`, `ifft2`, `ifftn`, `ifftshift`, `iqr`, `ischange`, `islocalmax`, `islocalmin`, `isoutlier`, `issortedrows`, `isuniform`, `mape`, `maxk`, `mink`, `movmad`, `movmax`, `movmean`, `movmedian`, `movmin`, `movprod`, `movstd`, `movsum`, `movvar`, `normalize`, `nufft`, `nufftn`, `pivot`, `prctile`, `quantile`, `rescale`, `rmmissing`, `rmoutliers`, `rmse`, `smoothdata`, `smoothdata2`, `splitapply`, `standardizeMissing`, `subspace`, `topkrows`, `trenddecomp`, `xcorr`, `xcov`

## matlab/datatypes

Implemented: 62. To implement: 46. In a branch: 0. Out of scope: 14.

To implement:

- [ ] `array2timetable`
- [ ] `calendarDuration`
- [ ] `categorical`
- [ ] `cell2mat`
- [ ] `cell2struct`
- [ ] `celldisp`
- [ ] `dictionary`
- [ ] `enumeration`
- [ ] `eventfilter`
- [ ] `events`
- [ ] `eventtable`
- [ ] `formattedDisplayText`
- [ ] `function_handle`
- [ ] `functions`
- [ ] `getfield`
- [ ] `inferiorto`
- [ ] `iscategorical`
- [ ] `isenum`
- [ ] `isordinal`
- [ ] `istabular`
- [ ] `istimetable`
- [ ] `leapseconds`
- [ ] `loadobj`
- [ ] `mat2cell`
- [ ] `namedargs2cell`
- [ ] `orderfields`
- [ ] `saveobj`
- [ ] `setfield`
- [ ] `struct2cell`
- [ ] `structfun`
- [ ] `superclasses`
- [ ] `superiorfloat`
- [ ] `superiorto`
- [ ] `swapbytes`
- [ ] `table2timetable`
- [ ] `tabular`
- [ ] `timerange`
- [ ] `timeseries2timetable`
- [ ] `timetable`
- [ ] `timetable2table`
- [ ] `timezones`
- [ ] `ts2timetable`
- [ ] `typecast`
- [ ] `underlyingType`
- [ ] `vartype`
- [ ] `withtol`

Out of scope (Java/.NET/COM/C/Perl interop or MEX): `iscom`, `isinterface`, `isjava`, `javaArray`, `javaMethod`, `javaMethodEDT`, `javaObject`, `javaObjectEDT`, `opaque`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `cellplot`, `methodsview`

Out of scope (internal helper, not a user function): `DefaultObject`, `internalArrayUtil`, `isUnderlyingType`

## matlab/elfun

Implemented: 70. To implement: 2. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `cospi`
- [ ] `sinpi`

## matlab/elmat

Implemented: 78. To implement: 4. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `allfinite`
- [ ] `anymissing`
- [ ] `anynan`
- [ ] `combinations`

## matlab/funfun

Implemented: 1. To implement: 52. In a branch: 0. Out of scope: 47.

To implement:

- [ ] `bvp4c`
- [ ] `bvp5c`
- [ ] `bvpget`
- [ ] `bvpinit`
- [ ] `bvpset`
- [ ] `bvpxtend`
- [ ] `dblquad`
- [ ] `dde23`
- [ ] `ddeget`
- [ ] `ddensd`
- [ ] `ddesd`
- [ ] `ddeset`
- [ ] `decic`
- [ ] `deval`
- [ ] `fcnchk`
- [ ] `inline`
- [ ] `inlineeval`
- [ ] `integral`
- [ ] `integral2`
- [ ] `integral3`
- [ ] `numjac`
- [ ] `ode`
- [ ] `ode113`
- [ ] `ode15i`
- [ ] `ode15s`
- [ ] `ode23`
- [ ] `ode23s`
- [ ] `ode23t`
- [ ] `ode23tb`
- [ ] `ode45`
- [ ] `ode78`
- [ ] `ode89`
- [ ] `odeEvent`
- [ ] `odeJacobian`
- [ ] `odeMassMatrix`
- [ ] `odeget`
- [ ] `odephas2`
- [ ] `odephas3`
- [ ] `odeplot`
- [ ] `odeprint`
- [ ] `odeset`
- [ ] `odextend`
- [ ] `pde`
- [ ] `pdepe`
- [ ] `pdeval`
- [ ] `quad`
- [ ] `quad2d`
- [ ] `quadgk`
- [ ] `quadl`
- [ ] `quadv`
- [ ] `triplequad`
- [ ] `vectorize`

Out of scope (MATLAB example file): `amp1dae`, `ballode`, `batonode`, `brussode`, `burgersode`, `ddex1`, `ddex1de`, `ddex1delays`, `ddex1hist`, `ddex2`, `ddex3`, `ddex4`, `ddex5`, `emdenbvp`, `fem1ode`, `fem2ode`, `fsbvp`, `hb1dae`, `hb1ode`, `iburgersode`, `ihb1dae`, `kneeode`, `lorenz`, `lotka`, `mat4bvp`, `odeexamples`, `orbitode`, `pdex1`, `pdex1bc`, `pdex1ic`, `pdex1pde`, `pdex2`, `pdex3`, `pdex4`, `pdex5`, `rcbvp`, `rigidode`, `shockbvp`, `threebvp`, `twobc`, `twobvp`, `twoode`, `vanderpoldemo`, `vdp1`, `vdp1000`, `vdpode`, `weissinger`

## matlab/general

Implemented: 6. To implement: 20. In a branch: 0. Out of scope: 44.

To implement:

- [ ] `cd`
- [ ] `clearvars`
- [ ] `genpath`
- [ ] `import`
- [ ] `isMATLABReleaseOlderThan`
- [ ] `load`
- [ ] `matlab`
- [ ] `matlabRelease`
- [ ] `matlabpath`
- [ ] `namelengthmax`
- [ ] `onCleanup`
- [ ] `save`
- [ ] `saveas`
- [ ] `toolboxdir`
- [ ] `type`
- [ ] `ver`
- [ ] `verLessThan`
- [ ] `which`
- [ ] `who`
- [ ] `whos`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `batchStartupOptionUsed`, `bench`, `callgraphviz`, `depfunprophelper`, `desktop`, `diary`, `echo`, `exit`, `findfile`, `finfo`, `getmcruserdata`, `inmem`, `isdeployed`, `ismcc`, `isstudent`, `logo`, `membrane`, `more`, `open`, `path2rc`, `preferences`, `quit`, `rehash`, `savepath`, `setmcruserdata`, `syntax`

Out of scope (Java/.NET/COM/C/Perl interop or MEX): `calllib`, `java`, `javaaddpath`, `javaclasspath`, `javarmpath`, `libfunctions`, `libfunctionsview`, `libisloaded`, `libpointer`, `libstruct`, `loadlibrary`, `mex`, `mexdebug`, `mexext`, `pcode`, `perl`, `unloadlibrary`, `usejava`

## matlab/graph2d

Implemented: 4. To implement: 6. In a branch: 0. Out of scope: 39.

To implement:

- [ ] `getcolumn`
- [ ] `loglog`
- [ ] `plotyy`
- [ ] `polar`
- [ ] `semilogx`
- [ ] `semilogy`

Out of scope (internal helper, not a user function): `arrowline`, `axischild`, `axisobj`, `axistext`, `basicfitdatastat`, `doclick`, `dokeypress`, `domymenu`, `doresize`, `editline`, `editrect`, `enddrag`, `fighandle`, `figobj`, `figtoolset`, `getobj`, `getorcreateobj`, `getscribecontextmenu`, `getscribeobjectdata`, `hgbin`, `jpropeditutils`, `middrag`, `moveaxis`, `prepdrag`, `putdowntext`, `scribeclearmode`, `scribeeventhandler`, `scribehandle`, `scribehgobj`, `scriberestoresavefcns`, `scribetextdlg`, `setscribecontextmenu`, `setscribeobjectdata`, `texlabel`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `gtext`, `pan`, `plotedit`, `rbbox`, `zoom`

## matlab/graph3d

Implemented: 3. To implement: 64. In a branch: 0. Out of scope: 8.

To implement:

- [ ] `alim`
- [ ] `alpha`
- [ ] `alphamap`
- [ ] `brighten`
- [ ] `camdolly`
- [ ] `camlight`
- [ ] `camlookat`
- [ ] `camorbit`
- [ ] `campan`
- [ ] `campos`
- [ ] `camproj`
- [ ] `camroll`
- [ ] `camrotate`
- [ ] `camtarget`
- [ ] `camup`
- [ ] `camva`
- [ ] `camzoom`
- [ ] `caxis`
- [ ] `cmpermute`
- [ ] `cmunique`
- [ ] `colormap`
- [ ] `diffuse`
- [ ] `fill3`
- [ ] `hidden`
- [ ] `imapprox`
- [ ] `lightangle`
- [ ] `lighting`
- [ ] `material`
- [ ] `mesh`
- [ ] `objbounds`
- [ ] `pbaspect`
- [ ] `plot3`
- [ ] `rlim`
- [ ] `rtickangle`
- [ ] `rtickformat`
- [ ] `rticklabels`
- [ ] `rticks`
- [ ] `shading`
- [ ] `specular`
- [ ] `surf`
- [ ] `surfl`
- [ ] `surfnorm`
- [ ] `thetalim`
- [ ] `thetatickformat`
- [ ] `thetaticklabels`
- [ ] `thetaticks`
- [ ] `view`
- [ ] `viewmtx`
- [ ] `xscale`
- [ ] `xtickangle`
- [ ] `xtickformat`
- [ ] `xticklabels`
- [ ] `xticks`
- [ ] `yscale`
- [ ] `ytickangle`
- [ ] `ytickformat`
- [ ] `yticklabels`
- [ ] `yticks`
- [ ] `zlim`
- [ ] `zscale`
- [ ] `ztickangle`
- [ ] `ztickformat`
- [ ] `zticklabels`
- [ ] `zticks`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `cameramenu`, `cameratoolbar`, `cmapeditor`, `colordef`, `colormapeditor`, `graymon`, `rotate3d`, `whitebg`

## matlab/graphics

Implemented: 17. To implement: 110. In a branch: 1. Out of scope: 84.

To implement:

- [ ] `abyss`
- [ ] `addpoints`
- [ ] `ancestor`
- [ ] `animatedline`
- [ ] `autumn`
- [ ] `axes`
- [ ] `bone`
- [ ] `bubblechart`
- [ ] `bubblechart3`
- [ ] `bubblecloud`
- [ ] `bubblelim`
- [ ] `bubblesize`
- [ ] `cla`
- [ ] `clearpoints`
- [ ] `clim`
- [ ] `colorcube`
- [ ] `colornone`
- [ ] `colororder`
- [ ] `cool`
- [ ] `copper`
- [ ] `copyobj`
- [ ] `dataTipTextRow`
- [ ] `datachildren`
- [ ] `datatip`
- [ ] `digraph`
- [ ] `donutchart`
- [ ] `etreeplot`
- [ ] `exportgraphics`
- [ ] `fcontour`
- [ ] `figuredeletedlistener`
- [ ] `fimplicit`
- [ ] `fimplicit3`
- [ ] `findall`
- [ ] `findobj`
- [ ] `flag`
- [ ] `fmesh`
- [ ] `fontname`
- [ ] `fontsize`
- [ ] `fplot3`
- [ ] `fsurf`
- [ ] `gcbf`
- [ ] `gcbo`
- [ ] `gco`
- [ ] `get`
- [ ] `getappdata`
- [ ] `getcrosssection`
- [ ] `getpoints`
- [ ] `gobjects`
- [ ] `gplot`
- [ ] `graph`
- [ ] `gray`
- [ ] `groot`
- [ ] `hggroup`
- [ ] `hgtransform`
- [ ] `hist`
- [ ] `histogram2`
- [ ] `hot`
- [ ] `hsv`
- [ ] `is2D`
- [ ] `isappdata`
- [ ] `isgraphics`
- [ ] `ishandle`
- [ ] `ishghandle`
- [ ] `jet`
- [ ] `light`
- [ ] `line`
- [ ] `lines`
- [ ] `makehgtform`
- [ ] `newplot`
- [ ] `nexttile`
- [ ] `orderedcolors`
- [ ] `parula`
- [ ] `patch`
- [ ] `piechart`
- [ ] `pink`
- [ ] `polaraxes`
- [ ] `polarbubblechart`
- [ ] `polarscatter`
- [ ] `polyshape`
- [ ] `prism`
- [ ] `rectangle`
- [ ] `refresh`
- [ ] `reset`
- [ ] `rmappdata`
- [ ] `scatter3`
- [ ] `set`
- [ ] `setappdata`
- [ ] `sky`
- [ ] `spring`
- [ ] `spy`
- [ ] `summer`
- [ ] `surf2patch`
- [ ] `surface`
- [ ] `swarmchart`
- [ ] `swarmchart3`
- [ ] `tex`
- [ ] `text`
- [ ] `tiledlayout`
- [ ] `tilenum`
- [ ] `tilerowcol`
- [ ] `treeplot`
- [ ] `turbo`
- [ ] `validatecolor`
- [ ] `white`
- [ ] `winter`
- [ ] `xsecondarylabel`
- [ ] `ysecondarylabel`
- [ ] `yyaxis`
- [ ] `zlabel`
- [ ] `zsecondarylabel`

In a branch: `alphaShape`

Out of scope (needs internet map tiles): `geoaxes`, `geobasemap`, `geobubble`, `geodensityplot`, `geolimits`, `geoplot`, `geoscatter`, `geotickformat`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `capturescreen`, `closereq`, `copygraphics`, `dataTipInteraction`, `datacursormode`, `dateTickPicker`, `disableDefaultInteractivity`, `editInteraction`, `enableDefaultInteractivity`, `enableLegacyExplorationModes`, `exportapp`, `exportsetupdlg`, `figureToolbarCreateFcn`, `figureheaderdlg`, `findprinters`, `getFigureToolManager`, `getprinttemplate`, `hgexport`, `hgload`, `hgsave`, `linkaxes`, `linkprop`, `openFigure`, `openfig`, `opengl`, `orient`, `pagesetupdlg`, `panInteraction`, `print`, `printdlg`, `printjob`, `printpreview`, `printtables`, `printtemplate`, `refreshdata`, `regionZoomInteraction`, `regionpicker`, `rendererinfo`, `resetplotview`, `rotateInteraction`, `rulerPanInteraction`, `saveFigure`, `savefig`, `setprinttemplate`, `shg`, `uiaxes`, `uicontextmenu`, `uicontrol`, `uimenu`, `vertexpicker`, `vga`, `zoomInteraction`

Out of scope (audio/video devices or screen capture): `getframe`

Out of scope (internal helper, not a user function): `axescheck`, `bwcontr`, `copyoptionsfcn`, `datetickstr`, `defaulterrorcallback`, `figcopytemplatelistener`, `figcopytemplatelistener_legacy`, `handle2struct`, `hasbehavior`, `hgaddbehavior`, `hgbehaviorfactory`, `hgclose`, `hgdisp`, `hgfeval`, `hggetbehavior`, `hggetdisp`, `hgrc`, `hgsetdisp`, `num2ruler`, `parseChartInputs`, `parseparams`, `ruler2num`, `struct2handle`

## matlab/imagesci

Implemented: 0. To implement: 4. In a branch: 0. Out of scope: 46.

To implement:

- [ ] `imfinfo`
- [ ] `imformats`
- [ ] `imread`
- [ ] `imwrite`

Out of scope (scientific file format (HDF/netCDF/CDF/FITS/TIFF) or web service): `Tiff`, `cdfepoch`, `cdfinfo`, `cdfread`, `cdfwrite`, `fitsdisp`, `fitsinfo`, `fitsread`, `fitswrite`, `h5create`, `h5disp`, `h5info`, `h5read`, `h5readatt`, `h5write`, `h5writeatt`, `hdf`, `hdf5`, `hdf5info`, `hdf5read`, `hdf5write`, `hdfan`, `hdfdf24`, `hdfdfr8`, `hdfh`, `hdfhd`, `hdfhe`, `hdfhx`, `hdfinfo`, `hdfml`, `hdfpt`, `hdfread`, `hdfv`, `hdfvf`, `hdfvh`, `hdfvs`, `multibandread`, `multibandwrite`, `nccreate`, `ncdisp`, `ncinfo`, `ncread`, `ncreadatt`, `ncwrite`, `ncwriteatt`, `ncwriteschema`

## matlab/iofun

Implemented: 4. To implement: 48. In a branch: 0. Out of scope: 12.

To implement:

- [ ] `csvread`
- [ ] `csvwrite`
- [ ] `dataread`
- [ ] `dlmread`
- [ ] `dlmwrite`
- [ ] `fclose`
- [ ] `feof`
- [ ] `ferror`
- [ ] `fgetl`
- [ ] `fgets`
- [ ] `filemarker`
- [ ] `fileparts`
- [ ] `fileread`
- [ ] `filesep`
- [ ] `fopen`
- [ ] `fread`
- [ ] `frewind`
- [ ] `fscanf`
- [ ] `fseek`
- [ ] `ftell`
- [ ] `fullfile`
- [ ] `fwrite`
- [ ] `home`
- [ ] `importdata`
- [ ] `matfile`
- [ ] `matlabroot`
- [ ] `pathsep`
- [ ] `prefdir`
- [ ] `readcell`
- [ ] `readmatrix`
- [ ] `readtimetable`
- [ ] `readvars`
- [ ] `tempdir`
- [ ] `tempname`
- [ ] `textread`
- [ ] `textscan`
- [ ] `timer`
- [ ] `timerfind`
- [ ] `timerfindall`
- [ ] `urlread`
- [ ] `urlwrite`
- [ ] `writecell`
- [ ] `writelines`
- [ ] `writematrix`
- [ ] `writetimetable`
- [ ] `xlsfinfo`
- [ ] `xlsread`
- [ ] `xlswrite`

Out of scope (scientific file format (HDF/netCDF/CDF/FITS/TIFF) or web service): `callSoapService`, `createClassFromWsdl`, `createSoapMessage`, `daqread`, `memmap_data_handle_holder`, `memmapfile`, `parseSoapResponse`, `sendmail`

Out of scope (internal helper, not a user function): `deployprefdir`, `matfinfo`, `partialpath`, `timercb`

## matlab/lang

Implemented: 7. To implement: 21. In a branch: 0. Out of scope: 7.

To implement:

- [ ] `assignin`
- [ ] `builtin`
- [ ] `clearAllMemoizedCaches`
- [ ] `details`
- [ ] `doclink`
- [ ] `dynamicprops`
- [ ] `eval`
- [ ] `evalc`
- [ ] `evalin`
- [ ] `genvarname`
- [ ] `handle`
- [ ] `input`
- [ ] `iskeyword`
- [ ] `isvarname`
- [ ] `localfunctions`
- [ ] `memoize`
- [ ] `message`
- [ ] `mfilename`
- [ ] `nargoutchk`
- [ ] `run`
- [ ] `validatestring`

Language keywords: `ans`, `arguments`, `break`, `case`, `catch`, `classdef`, `continue`, `else`, `elseif`, `empty`, `end`, `for`, `function`, `global`, `if`, `lists`, `nargin`, `nargout`, `otherwise`, `parallel_function`, `parfor`, `persistent`, `precedence`, `return`, `script`, `spmd_feval`, `switch`, `try`, `varargin`, `varargout`, `while`

Out of scope (internal helper, not a user function): `ParallelException`, `checkSyntacticWarnings`

Out of scope (Java/.NET/COM/C/Perl interop or MEX): `javachk`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `keyboard`, `mislocked`, `mlock`, `munlock`

## matlab/matfun

Implemented: 9. To implement: 6. In a branch: 47. Out of scope: 0.

To implement:

- [ ] `decomposition`
- [ ] `expmv`
- [ ] `ltitr`
- [ ] `pageinv`
- [ ] `pagenorm`
- [ ] `svdappend`

In a branch: `balance`, `bandwidth`, `cdf2rdf`, `chol`, `cholupdate`, `cond`, `condeig`, `condest`, `expm`, `funm`, `gsvd`, `hess`, `isbanded`, `ishermitian`, `issymmetric`, `ldl`, `linsolve`, `logm`, `lscov`, `lsqminnorm`, `lu`, `normest`, `normest1`, `null`, `ordeig`, `ordqz`, `ordschur`, `orth`, `pageeig`, `pagesvd`, `pinv`, `planerot`, `polyeig`, `qr`, `qrdelete`, `qrinsert`, `qrupdate`, `qz`, `rank`, `rcond`, `rref`, `rsf2csf`, `schur`, `sqrtm`, `svd`, `sylvester`, `vecnorm`

## matlab/ops

Implemented: 53. To implement: 4. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `ismembertol`
- [ ] `paren`
- [ ] `punct`
- [ ] `uniquetol`

## matlab/optimfun

Implemented: 0. To implement: 9. In a branch: 0. Out of scope: 10.

To implement:

- [ ] `fminbnd`
- [ ] `fminsearch`
- [ ] `fzero`
- [ ] `lsqnonneg`
- [ ] `optimget`
- [ ] `optimplotfunccount`
- [ ] `optimplotfval`
- [ ] `optimplotx`
- [ ] `optimset`

Out of scope (internal helper, not a user function): `addLink`, `callAllOptimOutputAndPlotFcns`, `callAllOptimOutputFcns`, `callAllOptimPlotFcns`, `createCellArrayOfFunctions`, `createProblemStruct`, `createSolverStruct`, `separateOptimStruct`, `validOptimProblemStruct`, `validateopts_UseParallel`

## matlab/polyfun

Implemented: 4. To implement: 9. In a branch: 31. Out of scope: 7.

To implement:

- [ ] `DelaunayTri`
- [ ] `TriRep`
- [ ] `TriScatteredInterp`
- [ ] `boundaryshape`
- [ ] `interp1q`
- [ ] `nsidedpoly`
- [ ] `polybuffer`
- [ ] `stlread`
- [ ] `stlwrite`

In a branch: `boundary`, `convhull`, `convhulln`, `delaunay`, `delaunayTriangulation`, `delaunayn`, `dsearchn`, `griddata`, `griddatan`, `griddedInterpolant`, `inpolygon`, `interp2`, `interp3`, `interpft`, `interpn`, `makima`, `mkpp`, `pchip`, `poly`, `polyarea`, `polyder`, `polyint`, `polyvalm`, `ppval`, `rectint`, `scatteredInterpolant`, `spline`, `triangulation`, `tsearchn`, `unmkpp`, `voronoin`

Out of scope (internal helper, not a user function): `automesh`, `pwch`, `qhull`, `splncore`, `xychk`, `xyzchk`, `xyzvchk`

## matlab/randfun

Implemented: 5. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `RandStream`

## matlab/scribe

Implemented: 1. To implement: 3. In a branch: 0. Out of scope: 13.

To implement:

- [ ] `annotation`
- [ ] `bubblelegend`
- [ ] `colorbar`

Out of scope (internal helper, not a user function): `defaultItemHitCallback`, `deselectall`, `getselectobjects`, `hgconvertunits`, `legendcolorbarlayout`, `legendinfo`, `scribealign`, `scribeccp`, `scribecursors`, `selectobject`, `snaptogrid`, `startscribeobject`, `startscribepinning`

## matlab/sparfun

Implemented: 1. To implement: 47. In a branch: 2. Out of scope: 0.

To implement:

- [ ] `amd`
- [ ] `bicg`
- [ ] `bicgstab`
- [ ] `bicgstabl`
- [ ] `cgs`
- [ ] `colamd`
- [ ] `colperm`
- [ ] `delsq`
- [ ] `dissect`
- [ ] `dmperm`
- [ ] `equilibrate`
- [ ] `etree`
- [ ] `full`
- [ ] `gmres`
- [ ] `ichol`
- [ ] `ilu`
- [ ] `issparse`
- [ ] `lsqr`
- [ ] `minres`
- [ ] `nested`
- [ ] `nonzeros`
- [ ] `numgrid`
- [ ] `nzmax`
- [ ] `pcg`
- [ ] `qmr`
- [ ] `rjr`
- [ ] `spalloc`
- [ ] `sparse`
- [ ] `spaugment`
- [ ] `spconvert`
- [ ] `spdiags`
- [ ] `speye`
- [ ] `spfun`
- [ ] `spones`
- [ ] `spparms`
- [ ] `sprand`
- [ ] `sprandn`
- [ ] `sprandsym`
- [ ] `sprank`
- [ ] `svdsketch`
- [ ] `symamd`
- [ ] `symbfact`
- [ ] `symmlq`
- [ ] `symrcm`
- [ ] `tfqmr`
- [ ] `treelayout`
- [ ] `unmesh`

In a branch: `eigs`, `svds`

## matlab/specfun

Implemented: 40. To implement: 1. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `matchpairs`

## matlab/specgraph

Implemented: 4. To implement: 91. In a branch: 1. Out of scope: 14.

To implement:

- [ ] `bar3`
- [ ] `bar3h`
- [ ] `barh`
- [ ] `binscatter`
- [ ] `boxchart`
- [ ] `clabel`
- [ ] `comet`
- [ ] `comet3`
- [ ] `compass`
- [ ] `coneplot`
- [ ] `contour`
- [ ] `contour3`
- [ ] `contourc`
- [ ] `contourf`
- [ ] `contours`
- [ ] `contourslice`
- [ ] `contrast`
- [ ] `curl`
- [ ] `cylinder`
- [ ] `dither`
- [ ] `divergence`
- [ ] `ellipsoid`
- [ ] `errorbar`
- [ ] `ezcontour`
- [ ] `ezcontourf`
- [ ] `ezgraph3`
- [ ] `ezmesh`
- [ ] `ezmeshc`
- [ ] `ezplot`
- [ ] `ezplot3`
- [ ] `ezpolar`
- [ ] `ezsurf`
- [ ] `ezsurfc`
- [ ] `feather`
- [ ] `fill`
- [ ] `heatmap`
- [ ] `image`
- [ ] `imagesc`
- [ ] `interpstreamspeed`
- [ ] `isocaps`
- [ ] `isocolors`
- [ ] `isonormals`
- [ ] `isosurface`
- [ ] `meshc`
- [ ] `meshz`
- [ ] `parallelplot`
- [ ] `pareto`
- [ ] `pcolor`
- [ ] `peaks`
- [ ] `pie`
- [ ] `pie3`
- [ ] `plotmatrix`
- [ ] `polarhistogram`
- [ ] `polarplot`
- [ ] `quiver`
- [ ] `quiver3`
- [ ] `reducepatch`
- [ ] `reducevolume`
- [ ] `rgbplot`
- [ ] `ribbon`
- [ ] `rose`
- [ ] `rotate`
- [ ] `scatterhistogram`
- [ ] `shrinkfaces`
- [ ] `slice`
- [ ] `smooth3`
- [ ] `sphere`
- [ ] `spinmap`
- [ ] `stackedplot`
- [ ] `stem3`
- [ ] `stream2`
- [ ] `stream3`
- [ ] `streamline`
- [ ] `streamparticles`
- [ ] `streamribbon`
- [ ] `streamslice`
- [ ] `streamtube`
- [ ] `subvolume`
- [ ] `surfc`
- [ ] `tetramesh`
- [ ] `trimesh`
- [ ] `triplot`
- [ ] `trisurf`
- [ ] `vissuite`
- [ ] `volumebounds`
- [ ] `waterfall`
- [ ] `wordcloud`
- [ ] `xline`
- [ ] `xregion`
- [ ] `yline`
- [ ] `yregion`

In a branch: `voronoi`

Out of scope (audio/video devices or screen capture): `frame2im`, `im2frame`, `movie`, `moviein`

Out of scope (Java/.NET/COM/C/Perl interop or MEX): `im2java`

Out of scope (internal helper, not a user function): `changeseriestype`, `colstyle`, `isplotchild`, `makebars`, `parseplotapi`, `plotchild`, `plotdoneevent`, `plotutils`, `usev6plotapi`

## matlab/strfun

Implemented: 54. To implement: 52. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `alphanumericBoundary`
- [ ] `alphanumericsPattern`
- [ ] `asFewOfPattern`
- [ ] `asManyOfPattern`
- [ ] `base2dec`
- [ ] `bin2dec`
- [ ] `caseInsensitivePattern`
- [ ] `caseSensitivePattern`
- [ ] `characterListPattern`
- [ ] `convertContainedStringsToChars`
- [ ] `dec2base`
- [ ] `dec2bin`
- [ ] `dec2hex`
- [ ] `digitBoundary`
- [ ] `digitsPattern`
- [ ] `eraseBetween`
- [ ] `extract`
- [ ] `findstr`
- [ ] `hex2dec`
- [ ] `hex2num`
- [ ] `isStringScalar`
- [ ] `isstr`
- [ ] `letterBoundary`
- [ ] `lineBoundary`
- [ ] `lookAheadBoundary`
- [ ] `lookBehindBoundary`
- [ ] `maskedPattern`
- [ ] `matches`
- [ ] `mustBeNonzeroLengthText`
- [ ] `mustBeText`
- [ ] `mustBeTextScalar`
- [ ] `namedPattern`
- [ ] `native2unicode`
- [ ] `num2hex`
- [ ] `optionalPattern`
- [ ] `pattern`
- [ ] `possessivePattern`
- [ ] `regexpPattern`
- [ ] `replaceBetween`
- [ ] `setstr`
- [ ] `splitlines`
- [ ] `sscanf`
- [ ] `str2mat`
- [ ] `strmatch`
- [ ] `strread`
- [ ] `strtok`
- [ ] `strvcat`
- [ ] `textBoundary`
- [ ] `unicode2native`
- [ ] `whitespaceBoundary`
- [ ] `whitespacePattern`
- [ ] `wildcardPattern`

## matlab/timefun

Implemented: 14. To implement: 18. In a branch: 0. Out of scope: 0.

To implement:

- [ ] `addtodate`
- [ ] `calendar`
- [ ] `clock`
- [ ] `cputime`
- [ ] `date`
- [ ] `datestr`
- [ ] `datetick`
- [ ] `datevec`
- [ ] `eomdate`
- [ ] `eomday`
- [ ] `etime`
- [ ] `lweekdate`
- [ ] `m2xdate`
- [ ] `months`
- [ ] `nweekdate`
- [ ] `timeit`
- [ ] `weeknum`
- [ ] `x2mdate`

## matlab/timeseries

Implemented: 0. To implement: 3. In a branch: 0. Out of scope: 39.

To implement:

- [ ] `event`
- [ ] `timeseries`
- [ ] `tscollection`

Out of scope (internal helper, not a user function): `BasicArray`, `SimTimeseries`, `ValueArray`, `VirtualArray`, `abstracttimemetadata`, `dataChangeEvent`, `hds`, `metadata`, `simulinkts2ts`, `timeseriesArray`, `tsAlignSizes`, `tsAnalyzeAbsTime`, `tsChkTime`, `tsIsDateFormat`, `tsIsSameTime`, `tsarrayFcn`, `tsdata`, `tsdateinterval`, `tsgetDateFormat`, `tsgetrelativetime`, `tsinterp`, `tsnaniqr`, `tsnanmax`, `tsnanmean`, `tsnanmedian`, `tsnanmin`, `tsnanmode`, `tsnanstd`, `tsnansum`, `tsnanvar`, `tsnewevent`, `tspnmatch`, `tsprctile`, `tspvformat`, `tssorttime`, `tsunitconv`, `utTrimNans`, `uttsget`, `variable`

## symbolic/symbolic

Implemented: 27. To implement: 187. In a branch: 0. Out of scope: 19.

To implement:

- [ ] `adjoint`
- [ ] `assumptions`
- [ ] `baseUnits`
- [ ] `bernoulli`
- [ ] `bernstein`
- [ ] `bernsteinMatrix`
- [ ] `catalan`
- [ ] `ccode`
- [ ] `cell2sym`
- [ ] `changeIntegrationVariable`
- [ ] `charToFunction`
- [ ] `charpoly`
- [ ] `chebyshevT`
- [ ] `chebyshevU`
- [ ] `checkUnits`
- [ ] `colspace`
- [ ] `combine`
- [ ] `coshint`
- [ ] `cosint`
- [ ] `daeFunction`
- [ ] `dawson`
- [ ] `derivedUnits`
- [ ] `dilog`
- [ ] `dirac`
- [ ] `displayFormula`
- [ ] `divisors`
- [ ] `dsolve`
- [ ] `ei`
- [ ] `eliminate`
- [ ] `ellipticCE`
- [ ] `ellipticCK`
- [ ] `ellipticCPi`
- [ ] `ellipticE`
- [ ] `ellipticF`
- [ ] `ellipticK`
- [ ] `ellipticNome`
- [ ] `ellipticPi`
- [ ] `equationsToMatrix`
- [ ] `erfi`
- [ ] `euler`
- [ ] `eulerPhi`
- [ ] `eulergamma`
- [ ] `factorIntegerPower`
- [ ] `fibonacci`
- [ ] `findDecoupledBlocks`
- [ ] `findSymType`
- [ ] `findUnits`
- [ ] `finverse`
- [ ] `fold`
- [ ] `fortran`
- [ ] `fourier`
- [ ] `frac`
- [ ] `fresnelc`
- [ ] `fresnels`
- [ ] `functionalDerivative`
- [ ] `gbasis`
- [ ] `gegenbauerC`
- [ ] `getEqnsVars`
- [ ] `harmonic`
- [ ] `has`
- [ ] `hasSymType`
- [ ] `heaviside`
- [ ] `hermiteForm`
- [ ] `hermiteH`
- [ ] `hessian`
- [ ] `horner`
- [ ] `htrans`
- [ ] `hurwitzZeta`
- [ ] `hypergeom`
- [ ] `ifourier`
- [ ] `igamma`
- [ ] `ihtrans`
- [ ] `ilaplace`
- [ ] `in`
- [ ] `incidenceMatrix`
- [ ] `integrateByParts`
- [ ] `isCondition`
- [ ] `isDistinctVariable`
- [ ] `isLowIndexDAE`
- [ ] `isPrimitiveRoot`
- [ ] `isSymType`
- [ ] `isUnit`
- [ ] `isVariable`
- [ ] `isolate`
- [ ] `iztrans`
- [ ] `jacobiAM`
- [ ] `jacobiCD`
- [ ] `jacobiCN`
- [ ] `jacobiCS`
- [ ] `jacobiDC`
- [ ] `jacobiDN`
- [ ] `jacobiDS`
- [ ] `jacobiNC`
- [ ] `jacobiND`
- [ ] `jacobiNS`
- [ ] `jacobiP`
- [ ] `jacobiSC`
- [ ] `jacobiSD`
- [ ] `jacobiSN`
- [ ] `jacobiSymbol`
- [ ] `jacobiZeta`
- [ ] `jordan`
- [ ] `kroneckerDelta`
- [ ] `kummerU`
- [ ] `laguerreL`
- [ ] `lambertw`
- [ ] `laplace`
- [ ] `laplacian`
- [ ] `legendreP`
- [ ] `logint`
- [ ] `mapSymType`
- [ ] `massMatrixForm`
- [ ] `mathml`
- [ ] `meijerG`
- [ ] `minpoly`
- [ ] `mixedUnits`
- [ ] `mustBeInteger`
- [ ] `mustBePositive`
- [ ] `newUnit`
- [ ] `newUnitSystem`
- [ ] `nextprime`
- [ ] `nthprime`
- [ ] `odeFunction`
- [ ] `odeToVectorField`
- [ ] `pade`
- [ ] `partfrac`
- [ ] `pdeCoefficients`
- [ ] `pdeCoefficientsToDouble`
- [ ] `piecewise`
- [ ] `pochhammer`
- [ ] `poles`
- [ ] `polylog`
- [ ] `polynomialDegree`
- [ ] `polynomialReduce`
- [ ] `potential`
- [ ] `powermod`
- [ ] `prevprime`
- [ ] `quorem`
- [ ] `rectangularPulse`
- [ ] `reduceDAEIndex`
- [ ] `reduceDAEToODE`
- [ ] `reduceDifferentialOrder`
- [ ] `reduceRedundancies`
- [ ] `removeUnit`
- [ ] `removeUnitSystem`
- [ ] `resultant`
- [ ] `rewrite`
- [ ] `root`
- [ ] `rsums`
- [ ] `separateUnits`
- [ ] `series`
- [ ] `signIm`
- [ ] `simplifyFraction`
- [ ] `sinc`
- [ ] `sinhint`
- [ ] `sinint`
- [ ] `smithForm`
- [ ] `ssinint`
- [ ] `str2sym`
- [ ] `str2symunit`
- [ ] `subexpr`
- [ ] `sym2cell`
- [ ] `symType`
- [ ] `symbolicUnitsCollection`
- [ ] `symfalse`
- [ ] `symfunmatrix`
- [ ] `symmatrix`
- [ ] `sympref`
- [ ] `symprod`
- [ ] `symsum`
- [ ] `symtrue`
- [ ] `symunit`
- [ ] `symunit2str`
- [ ] `triangularPulse`
- [ ] `unitConversionFactor`
- [ ] `unitConvert`
- [ ] `unitInfo`
- [ ] `unitSystems`
- [ ] `vectorPotential`
- [ ] `vpaintegral`
- [ ] `vpasolve`
- [ ] `vpasum`
- [ ] `whittakerM`
- [ ] `whittakerW`
- [ ] `wrightOmega`
- [ ] `zeta`
- [ ] `ztrans`

Out of scope (internal helper, not a user function): `checkIgnoreAnalyticConstraintsValue`, `symFunType`

Out of scope (desktop, IDE, GUI, printing or interactive tool): `allMuPADNotebooks`, `convertMuPADNotebook`, `funtool`, `matlabFunctionBlock`, `mupad`, `mupadengine`, `mupadwelcome`, `openmn`, `openxvc`, `openxvz`, `release`, `simscapeEquation`, `symReadSSCParameters`, `symReadSSCVariables`, `symWriteSSC`, `symengine`, `taylortool`


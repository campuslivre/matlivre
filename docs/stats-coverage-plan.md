# Statistics and Machine Learning Toolbox: coverage plan

How MatLivre grows to cover the MATLAB Statistics and Machine Learning Toolbox, the way the Signal
Processing Toolbox was done (`docs/signal-coverage-plan.md`). The function inventory is
`tools/coverage/stats.tsv` (category, name); the object methods are in
`tools/coverage/stats_methods.tsv`.

## Where the inventory comes from

- The toolbox's table of contents (`mathworks.com/help/stats/helpfuncbycat.xml`, the same file the
  installed products ship as `help/<product>/helpfuncbycat.xml`) gives 1787 entries. It is the
  R2026b listing.
- The release each page says it was introduced in ("Introduced in R20xxx" on the function's page)
  was read for each page. The 35 top-level functions introduced in R2024a or later are left out
  (`hnswSearcher`, `incrementalPCA`, `permutationImportance`, `fitlmcens`, `umap`, `mmdtest`,
  `knntest`, `rjtest`, `swtest`, `pearsinv`, `incrementalKMeans`, `smoteTabularSynthesizer`, the
  `*DOE` design functions, `templateNeuralNetwork`, `fitrqlinear`, ...), because MatLivre targets
  R2023b. Entries pointing at core MATLAB pages (`rand`, ...) are dropped.
- Entries that are methods of a class (`predict (ClassificationTree)`, `linearmodel.plotResiduals`,
  ...) are not in `stats.tsv`; they are 510 distinct class.method pages in `stats_methods.tsv` and
  are implemented with their class.
- The reference's table of contents lists a function once per category it appears in and omits the
  generic `pdf`, `cdf`, `icdf` and `random`; those four were added by hand (category "Generic
  distribution functions"). `prctile`, `quantile`, `iqr`, `mode` and `normalize` are core MATLAB
  in R2023b and already tracked there.
- The category column is the first place the table of contents lists a name, which is not always
  the natural one (`pdist` sits under "Nearest Neighbors", `ttest` under the distributions). The
  priorities below group by topic, not by that column.

The coverage checklist (`tools/coverage/update.sh`, `coverage.pl`) reads `stats.tsv` and adds a
"Statistics and Machine Learning Toolbox" section to `docs/matlab-coverage.md`.

## Where things stand

| | Count |
|---|---:|
| Functions and classes in scope list (`stats.tsv`, R2023b-era) | 485 |
| Out of scope (see below) | 15 |
| In scope | 470 |
| Implemented in MatLivre (`crates/livre-lang/src/stats/`) | 260 |
| Class methods (`stats_methods.tsv`), implemented with their classes | 510 |

Names per table-of-contents category (first listing): Probability distributions and hypothesis
tests 217 (119 continuous and 39 discrete distribution functions, 24 hypothesis tests, 16
multivariate, 4 exploration and plots, 7 resampling, 4 random number generation, 4 generic),
Regression 75, Descriptive statistics and visualization 60, Classification 48, Cluster analysis and
anomaly detection 28, Industrial statistics 27, Dimensionality reduction 14, ANOVA 10, Simulink
and code generation 6.

The Statistics and Machine Learning Toolbox is not installed in the MATLAB on this machine (R2023b
has Signal, Wavelet, Audio, DSP System and Symbolic Math only; `exist('normcdf')`, `exist('fitlm')`
and `exist('kmeans')` are 0). Nothing can be recorded, so **no implementation starts until the
toolbox is installed** (check with `matlab -batch "disp(exist('normcdf'))"`: 2 or 6 means
installed). Implementing from memory is not done: AGENTS.md requires recorded output.

## Out of scope

- Simulink and code generation: `saveLearnerForCoder`, `loadLearnerForCoder`,
  `learnerCoderConfigurer`, `generateLearnerDataTypeFcn`, `saveCompactModel`, `loadCompactModel`.
- Incremental learning (streaming, object-heavy): the 8 functions in the two "Incremental Learning"
  categories (`incrementalClassificationLinear`, `incrementalDriftAwareLearner`, ...).
- The Polynomial Fitting Tool and every app (Distribution Fitter, Regression Learner,
  Classification Learner, Probability Distribution Function, Random Number Generation).
- Options and data types, not functions, that follow the existing scope rules: tall arrays, GPU
  arrays, `UseParallel` and distributed computing (the option is accepted and ignored), code
  generation, Bayesian-optimisation hyperparameter search (`bayesopt`, `OptimizeHyperparameters`,
  `fitcauto`, `fitrauto`) until the base learners exist.
- Deep learning layers and datastores: none are listed in this toolbox; `fitcnet` and `fitrnet`
  (shallow fully-connected networks) stay in scope at the lowest priority.
- Not planned: `dataset`, `nominal`, `ordinal` and their methods (listed "Not Recommended"; tables and
  `categorical` are the supported types), `xptread` (SAS file format).

## Priority order

Within each step the order is the one in the list; every function is recorded against MATLAB
before it counts as done.

1. **Descriptive statistics.** `geomean`, `harmmean`, `trimmean`, `kurtosis`, `skewness`,
   `moment`, `range`, `mad`, `zscore`, `corr`, `partialcorr`, `partialcorri`, `tiedrank`,
   `crosstab`, `tabulate`, `grpstats`, `grp2idx`, `dummyvar`, `onehotencode`, `onehotdecode`,
   `robustcov`, `cholcov`, `corrcov`, `nearcorr`, `nan*` functions, `ecdf`, `ksdensity`, `hist3`,
   `histfit`; plots after the numbers (`boxplot`, `qqplot`, `normplot`, `scatterhist`,
   `gscatter`, `gplotmatrix`, `parallelcoords`, `biplot`).
2. **Probability distributions.** The `pdf/cdf/inv/like/stat/fit/rnd` families (about 150
   functions: normal, lognormal, gamma, beta, chi-square, F, t, exponential, Weibull, extreme value,
   generalized extreme value and Pareto, Rayleigh, uniform, noncentral variants, Poisson,
   binomial, geometric, hypergeometric, negative binomial, discrete uniform), then the
   generic `pdf`, `cdf`, `icdf`, `random`, `mle`, `mlecov`, `makedist`, `fitdist` and the
   `prob.*Distribution` objects (methods `pdf cdf icdf random mean var std median iqr truncate
   negloglik paramci`), `paretotails`, `pearsrnd`, `johnsrnd`, the multivariate normal and t, the
   multinomial, `wishrnd`, copulas, `randg`, `randsample`, `datasample`, `bootstrp`, `bootci`,
   `jackknife`, `slicesample`, `mhsample`.
3. **Hypothesis tests.** `ttest`, `ttest2`, `ztest`, `vartest`, `vartest2`, `vartestn`, `kstest`,
   `kstest2`, `lillietest`, `adtest`, `jbtest`, `chi2gof`, `signtest`, `signrank`, `ranksum`,
   `runstest`, `fishertest`, `kruskalwallis`, `friedman`, `ansaribradley`, `barttest`, `dwtest`,
   `sampsizepwr`, `multcompare`, `testcholdout`, `testckfold`, `meanEffectSize`.
4. **Linear and generalized linear regression.** `regress`, `regstats`, `robustfit`,
   `stepwisefit`, `polyconf`, `x2fx`, `lasso`, `ridge`, `plsregress`, `fitlm` with the
   `LinearModel` object (coefficients table, `anova`, `predict`, `feval`, `coefCI`, `coefTest`,
   `plotResiduals`, `step`, `addTerms`, `removeTerms`, display), `stepwiselm`, `fitglm`,
   `GeneralizedLinearModel`, `stepwiseglm`, `glmfit`, `glmval`, `mnrfit`, `mnrval`, `lassoglm`,
   `nlinfit`, `nlparci`, `nlpredci`, `fitnlm`, `mvregress`, `leverage`, then mixed-effects
   (`fitlme`, `fitglme`) last.
5. **ANOVA.** `anova1`, `anova2`, `anovan`, `multcompare`, `kruskalwallis`, `friedman`,
   `canoncorr`, `manova1`, `fitrm` and `ranova`, `manova`.
6. **Clustering.** `pdist`, `pdist2`, `squareform`, `linkage`, `cluster`, `clusterdata`, `cophenet`,
   `inconsistent`, `dendrogram` (the numeric `[h,t,perm]` outputs), `optimalleaforder`, `kmeans`
   (the algorithm is seed dependent: match under `rng` only for the initial centroids that follow
   from the documented rules), `kmedoids`, `silhouette`, `evalclusters`, `dbscan`,
   `fitgmdist`/`gmdistribution`, `spectralcluster`, `mahal`, `knnsearch`, `rangesearch`,
   `createns`; `hmm*` last.
7. **Dimensionality reduction.** `pca`, `pcacov`, `pcares`, `ppca`, `factoran`, `rotatefactors`,
   `nnmf`, `cmdscale`, `mdscale`, `procrustes`, `tsne` (seed dependent), `rica`, `sparsefilt`,
   `fsulaplacian`, `relieff`, `sequentialfs`.
8. **Classification and regression-learner basics.** `cvpartition`, `crossval`, `confusionmat`,
   `perfcurve`, `rocmetrics`, then `fitcknn`, `fitctree`, `fitrtree`, `fitcnb`, `fitcdiscr`,
   `classify`, `fitcsvm`, `fitrsvm`, `fitcecoc`, `fitcensemble`, `fitrensemble`, `TreeBagger`, with
   the model methods (`predict`, `resubPredict`, `kfoldPredict`, `loss`, `margin`, `crossval`,
   `compact`, `view` data), `fitrgp`, `fitcgam`, `fitrgam`, `fitclinear`, `fitrlinear`,
   `fitckernel`, `fitcnet`, `fitrnet`, `iforest`, `lof`, `ocsvm`, feature selection (`fscmrmr`,
   `fscnca`, `fsrnca`, `fsrftest`), interpretability (`lime`, `shapley`), fairness.
9. **Random number generation, design of experiments and process control.** `lhsdesign`,
   `lhsnorm`, `haltonset`, `sobolset`, `qrandstream`, `fullfact`, `ff2n`, `fracfact`, `bbdesign`,
   `ccdesign`, `rowexch`, `cordexch`, `candexch`, `daugment`, `dcovary`, `capability`,
   `controlchart`, `controlrules`, `gagerr`, `coxphfit`, `fitcox`.

## How it will be tested (once the toolbox is installed)

Like `signal_matlab.rs`:

- `tools/stats/gen_cases.py` and `tools/stats/cases.txt` build `stats_cases.m`; MATLAB's output is
  recorded in `crates/livre-lang/tests/all/data/stats_matlab.txt`; the test
  `stats_matlab.rs` compares classes, sizes and messages exactly and numbers to a relative 1e-9
  (1e-6 single). Differential cases under `tests/matlab/cases/stats/` cover display formats and
  objects (`LinearModel` displays, `prob.NormalDistribution` displays).
- A `stats_known_deviations.tsv` lists the cases still different, each with a reason. The test
  fails when a listed case starts to match, so the list only shrinks. The may-match marker is used
  only for entries that depend on the platform (random number streams, solver rounding).
- Random functions: first check which generators `rng` seeds in MATLAB (`twister`, `simdTwister`,
  `philox`, `threefry`) are reproduced by MatLivre; `normrnd`/`random` follow `randn`, but
  algorithms with their own sampling order (`gamrnd`, `betarnd`, `binornd`, `randsample`,
  `datasample`, `kmeans` seeding, `cvpartition`, `tsne`) have to be recovered from MATLAB's output.
- Code lives in `crates/livre-lang/src/stats/`, one module per area, with a `NAMES` list per module
  and `stats::call` as the router, wired in `builtins.rs` like `signal`.

## Risks

- **Toolbox not installed**: everything past this document is blocked.
- **Seed dependent algorithms**: `kmeans`, `kmedoids`, `tsne`, `fitgmdist`, `cvpartition`, bootstrap,
  `TreeBagger` and the random variate generators match MATLAB only if the exact sampling procedure
  and stream consumption are recovered; some will end up as listed deviations (as `emd`/`vmd` did
  for signal).
- **Iterative optimisers** (`mle`, `fitdist`, `fitglm`, `nlinfit`, `fitcsvm`, `fitrgp`, `lasso`):
  MATLAB stops at a tolerance, so parameters agree only to the stopping tolerance (as `fircls`
  did); expect a 1e-6 to 1e-9 comparison per function and per-function tolerances.
- **Object displays**: `LinearModel`, `GeneralizedLinearModel`, `ClassificationTree` and
  `prob.*Distribution` have long formatted displays and a `Properties` list that must match
  exactly; they depend on the table and `categorical` types already being complete.
- **Size**: about 485 functions plus 510 methods, three times the Signal toolbox; the priority list
  above is meant to be shipped in batches of 30 to 60 names, each recorded and committed before
  the next.
- **Dependencies**: many functions take table or `categorical` input, formula strings
  (`'y ~ x1 + x2'`) and name-value options; the formula parser (Wilkinson notation) is shared by
  `fitlm`, `fitglm`, `fitlme`, `anovan` and `fitrm` and should be built once, early in step 4.
- **Methods inventory**: `stats_methods.tsv` has the methods the reference lists; the classes
  themselves (`LinearModel`, `ClassificationTree`, ...) are not top-level entries in the
  reference's table of contents, so they are not counted in the 485.

## Status

MATLAB R2023b with the toolbox is installed. Done and recorded (`tools/stats/cases*.txt`, 1900 cases): descriptive statistics, `corr`, pdf/cdf/inv/stat of 20 families, rnd of the `rand`-based families, `ttest`/`ttest2`/`ztest`/`vartest`/`vartest2`, `pdist`/`pdist2`/`squareform`/`mahal`, `linkage`/`cluster`/`clusterdata`/`cophenet`/`inconsistent`, `pca`/`pcacov`/`pcares`/`cmdscale`/`corrcov`, `regress`/`dummyvar`/`x2fx`. Not recovered: `randn`'s Ziggurat (so `normrnd`, `gamrnd` and the other randn/randg-based generators). Next: `kmeans`, `fitlm`/`fitglm`, `anova1`, nonparametric tests, `fit`/`like`/`mle`, `makedist`/`fitdist`, learners.

Update: `kmeans` (sqeuclidean, cosine, correlation), `randsample`, `datasample`, `signtest`, `signrank`, `ranksum`, `anova1`, `kruskalwallis`, `normfit`/`expfit`/`poissfit`/`binofit`/`lognfit`/`unifit`/`raylfit`, `normlike`/`lognlike`/`explike` are done (2199 recorded cases). Next: `fitlm`/`fitglm`, `kstest`/`kstest2`, `fitdist`/`makedist`, learners.

Update 2: `kstest`, `kstest2` and `fitlm` (matrix input, LinearModel properties, display, predict, feval, coefCI) are done (2322 recorded cases). Not done in fitlm: table/formula input, weights, categorical predictors, rank-deficient designs, anova. Next: fitglm, fitdist/makedist, learners.

Update 3: `fitglm` (matrix input; GeneralizedLinearModel properties, display, predict, feval, coefCI) done (2383 recorded cases). Next: fitdist/makedist, classification learners.

Update 4 (4672 recorded cases in `tools/stats/cases*.txt`, `cases_u1.txt` is the newest): `jbtest`, `dwtest`, `chi2gof`, `vartestn`, `ansaribradley`, `lillietest` (hypothesis tests), `glmfit`, `capability`, `hist3`, `controlrules`, `fracfact` and `ccdesign` are done and recorded against R2023b (244 implemented, 226 of the 470 in scope left). Critical-value tables of `jbtest` and `lillietest` were recovered from MATLAB's output, not copied. Deviations listed in `stats_known_deviations.tsv`: single-precision p-value of `lillietest`, single-precision `capability` Cp and `glmfit`, and the `weights`/`offset` options of `glmfit` (not implemented). Not done in this round: `adtest` (large tables and an asymptotic distribution), `multcompare` (needs the studentized range distribution `stdrcdf`/`stdrinv`), `anovan`, `mle`/`mlecov`, `cvpartition`, `fitctree`, the `*rnd` functions (`randn`'s Ziggurat is still not recovered) and the `jbtest`/`lillietest` `MCTol` Monte Carlo path (random, so it cannot match). `vartestn` shows no table or figure with `'on'`. Next: `multcompare` (after `stdrcdf`), `anovan`, `adtest`, `mle`, `cvpartition`, `fitctree`.

Update 5 (5818 recorded cases, `cases_u1.txt` is still the newest): `multcompare` (after `anova1`, `anova2`, `kruskalwallis`, `friedman` and `anovan`; the studentized range distribution of AS 190 is inside it, and the Tukey-Kramer, Dunn-Sidak, Bonferroni, LSD and Scheffe critical values and p-values are recorded), `anovan` (sum-to-zero dummy variables with the constraint matrices of MATLAB, sums of squares of type 1, 2, 3 and hierarchical, continuous covariates, custom terms, `stats` with `Rtr`, `rowbasis` and `nullproject`), `adtest` (the composite tests: normal, exponential, extreme value, lognormal, Weibull; the critical-value coefficients were recovered from MATLAB's output), `hougen`, `ecdfhist`, `testcholdout`, `invpred`, `partialcorri`, `regstats`, `stepwisefit`, `plsregress`, `hmmtrain`, `bootstrp`, `candgen`, `bootci` and `lhsdesign` are done and recorded against R2023b (260 implemented, 210 of the 470 in scope left). `rand`, `randi` and `randperm` give MATLAB's streams, so `bootstrp`, `bootci` and `lhsdesign` match call by call with `rng`; `randn` still does not. `tcdf`, `tinv`, `fcdf` and `finv` now accept infinite degrees of freedom. Known deviations (`stats_known_deviations.tsv`): `multcompare` with `CType` `dunnett` for three or more groups needs `mvtcdf`; `anovan` options `random` and `nested`, the coefficients of rank-deficient designs and single input; `plsregress` loadings, scores and weights follow the sign convention of MatLivre's `svd` (faer), which differs from LAPACK's, so the cases record absolute values and the sign-free outputs; `bootstrp` with `Weights`; single-precision arithmetic of `adtest`, `regstats`, `stepwisefit` and `plsregress`; rank-deficiency warnings of `partialcorri`. Blocked or not done: `cvpartition` (a class with `training`, `test` and `repartition`), `gagerr` and the `random` option of `anovan`, `mle` (833 lines of distribution handling), `ksdensity` (boundary corrections and bandwidth rules in private helpers), `nearcorr` (Newton method with `minres`), `rotatefactors` (`svd` signs), `fracfactgen` (a backtracking search), `fitctree`, and the `*rnd` functions that use `randn`. Next: a LAPACK-compatible `svd` (unblocks `plsregress`, `rotatefactors`, `factoran`), `cvpartition` with `crossval`, `mle`, `ksdensity`, `fitctree`.

Update 6 (6034 recorded cases, `cases_u1.txt` is still the newest): `cvpartition` is done and recorded against R2023b (261 implemented, 209 of the 470 in scope left): K-fold, hold-out, leave-one-out and resubstitution, stratified when given group labels (numeric, logical, character matrix, cellstr, string and categorical groups, missing groups kept as NaN), `'Stratify'`, the `training`, `test`, `repartition` and `gather` methods, the display, the read-only properties and the error messages. The partitions are drawn with `rand`, so under `rng(seed)` every fold matches MATLAB's. `grp2idx` now takes a character matrix (its rows are the labels). Not done: the `RandStream` argument of `cvpartition` and `repartition`, `'CustomPartition'` (`IsCustom` true), and tall arrays. Next: `crossval`, `mle`, `ksdensity`, `fitctree`, a LAPACK-compatible `svd`.

Update 7 (6251 recorded cases, `cases_u1.txt` is still the newest): `crossval` is done and recorded against R2023b (262 implemented, 208 of the 470 in scope left): a function handle of the training and test rows (one row of results per fold, in the class of the first result, cell results too), `'mse'` and `'mcr'` with `'Predfun'`, `'KFold'`, `'HoldOut'`, `'LeaveOut'`, `'Partition'`, `'Stratify'` and `'MCReps'`, the partial names of `parseArgs`, and the errors and warnings of MATLAB. The partitions come from `cvpartition` and `repartition`, so under `rng(seed)` every fold, and a `rand` drawn inside the function, match MATLAB's. Also fixed along the way: `polyfit` with inputs of different sizes is an error (it panicked), `numel(A, idx...)`, `repmat` of a categorical array, the grouping error text of `grp2idx` for a cell that is not text, and `'mcr'` compares the labels of logical values apart from numeric ones. Not done: the parallel and stream settings of `'Options'` (checked, then ignored), tables as data, and an anonymous function that is only a call to `error` is recognised by its text. Next: `mle`, `ksdensity`, `fitctree`, a LAPACK-compatible `svd`.

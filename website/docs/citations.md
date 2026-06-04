# References & Citations

Every ecological index, statistical test, and ordination method in coralX is listed here
with a plain-English explanation of what it measures and its primary citation.

---

## Coverage & Confidence Intervals

### Wilson Score Interval — per-code 95% CI
**What it tells you:** How precisely the observed coverage % estimates the true proportion,
accounting for small sample sizes or values near 0% or 100% where the standard normal
approximation breaks down. Bars in the Coverage chart show these bounds.

> Wilson, E. B. (1927). Probable inference, the law of succession, and statistical inference.
> *Journal of the American Statistical Association*, 22(158), 209–212.
> https://doi.org/10.1080/01621459.1927.10502953

---

## Reef Health

### Reef Health Classification — live coral cover thresholds
**What it tells you:** A standardised four-level label (Poor / Fair / Good / Excellent) based on
the percentage of live hard coral cover. Used by reef managers across the Indo-Pacific to quickly
communicate reef condition and track change over time.

| Category | Live hard coral cover |
|---|---|
| Poor | 0 – < 25 % |
| Fair | 25 – < 50 % |
| Good | 50 – < 75 % |
| Excellent | 75 – 100 % |

> Gomez, E. D., & Yap, H. T. (1988). Monitoring reef condition.
> In R. A. Kenchington & B. E. T. Hudson (Eds.),
> *Coral Reef Management Handbook* (pp. 187–195). UNESCO Regional Office, Jakarta.

Also codified in Indonesia as *Keputusan Menteri Lingkungan Hidup No. 4 Tahun 2001*.

---

### Mortality Index (MI)
**What it tells you:** The proportion of dead coral relative to all coral (live + dead).
MI = dead coral points / (live hard coral + dead coral points).
- MI ≈ 0 → almost no dead coral; reef is healthy.
- MI ≈ 1 → near-total mortality; recent bleaching, disease, or physical damage likely.

Useful for detecting acute disturbance events when surveyed shortly after impact.

> English, S., Wilkinson, C., & Baker, V. (Eds.). (1997).
> *Survey Manual for Tropical Marine Resources* (2nd ed.).
> Australian Institute of Marine Science, Townsville.

---

### Coral : Algae Ratio
**What it tells you:** Whether the reef is coral-dominated or algae-dominated.
Ratio = live hard coral % / algae %.
- Ratio > 1 → coral outcompetes algae; reef is in good competitive state.
- Ratio < 1 → algae dominates; may indicate a phase shift driven by reduced herbivory,
  elevated nutrients, or post-disturbance recovery failure.

> Hughes, T. P. (1994). Catastrophes, phase shifts, and large-scale degradation of a
> Caribbean coral reef. *Science*, 265(5178), 1547–1551.
> https://doi.org/10.1126/science.265.5178.1547

> Bellwood, D. R., Hughes, T. P., Folke, C., & Nyström, M. (2004).
> Confronting the coral reef crisis. *Nature*, 429, 827–833.
> https://doi.org/10.1038/nature02691

---

## Diversity Indices

### Shannon Diversity Index (H′)
**What it tells you:** Overall community diversity, sensitive to both the number of different
benthic codes present and how evenly points are distributed among them.
H′ = 0 means only one code is present; higher values indicate more diverse communities.
Useful for comparing sites or tracking change over monitoring periods.

> Shannon, C. E., & Weaver, W. (1949).
> *The Mathematical Theory of Communication*.
> University of Illinois Press, Urbana.

---

### Simpson's Diversity Index (1 − D)
**What it tells you:** The probability that two randomly chosen points belong to different
benthic categories. Ranges 0–1; values near 1 indicate high diversity.
Less sensitive to rare codes than Shannon; more weight given to dominant categories.

> Simpson, E. H. (1949). Measurement of diversity.
> *Nature*, 163, 688.
> https://doi.org/10.1038/163688a0

---

### Pielou's Evenness (J′)
**What it tells you:** How evenly points are spread across categories, independent of
how many categories there are. J′ = 1 means all codes are equally abundant.
Low J′ with high richness means one or two codes dominate. Helps interpret H′:
a high H′ driven by evenness has different ecological meaning than one driven by richness.

> Pielou, E. C. (1966). The measurement of diversity in different types of biological collections.
> *Journal of Theoretical Biology*, 13, 131–144.
> https://doi.org/10.1016/0022-5193(66)90013-0

---

### Margalef's Species Richness (d)
**What it tells you:** The number of distinct benthic codes corrected for sample size.
d = (S − 1) / ln(N). Allows fair comparison of richness between images or stations that
have different total point counts.

> Margalef, R. (1958). Information theory in ecology.
> *General Systems*, 3, 36–71.

---

### Fisher's Alpha (α)
**What it tells you:** A parametric diversity index that is relatively robust to
differences in sampling effort, making it useful for long-term monitoring where
point counts may vary between surveys. Solved numerically from S = α ln(1 + N/α).

> Fisher, R. A., Corbet, A. S., & Williams, C. B. (1943).
> The relation between the number of species and the number of individuals in a random
> sample of an animal population.
> *Journal of Animal Ecology*, 12, 42–58.
> https://doi.org/10.2307/1411

---

### Berger–Parker Dominance (d)
**What it tells you:** The proportion of points belonging to the single most abundant
benthic code. d = n\_max / N. A simple, intuitive dominance measure: d = 1 means
one code accounts for all points; low d means community is broadly distributed.

> Berger, W. H., & Parker, F. L. (1970).
> Diversity of planktonic foraminifera in deep-sea sediments.
> *Science*, 168(3937), 1345–1347.
> https://doi.org/10.1126/science.168.3937.1345

---

### Hill Numbers (q0, q1, q2)
**What it tells you:** A unified family of diversity metrics expressed as "effective number
of species" — the number of equally abundant categories that would produce the same index value.

| Order | Formula | Sensitive to |
|---|---|---|
| q0 | Species richness (S) | Rare categories equally as much as common ones |
| q1 | exp(Shannon H′) | Intermediate — weighted by abundance |
| q2 | 1 / Simpson D | Dominant categories most |

When q0 ≈ q1 ≈ q2 the community is highly even. A large drop from q0 to q2 reveals that
a few categories dominate despite high raw richness.

> Hill, M. O. (1973). Diversity and evenness: a unifying notation and its consequences.
> *Ecology*, 54(2), 427–432.
> https://doi.org/10.2307/1934352

> Jost, L. (2006). Entropy and diversity.
> *Oikos*, 113(2), 363–375.
> https://doi.org/10.1111/j.2006.0030-1299.14714.x

---

## Statistical Tests

### Bootstrap Confidence Intervals
**What it tells you:** Uncertainty bounds around diversity indices (Shannon, Simpson) computed
by repeatedly resampling the labeled points with replacement. Does not assume normality, making
it reliable for small datasets or highly skewed communities.

> Efron, B. (1979). Bootstrap methods: Another look at the jackknife.
> *Annals of Statistics*, 7(1), 1–26.
> https://doi.org/10.1214/aos/1176344552

> Efron, B., & Tibshirani, R. J. (1993).
> *An Introduction to the Bootstrap*.
> Chapman & Hall/CRC, New York.

---

### One-way ANOVA / Kruskal–Wallis Test
**What it tells you:** Whether a chosen metric (e.g., live coral cover %) differs
significantly across groups of stations (e.g., protected vs. unprotected zones, depth bands).
- **ANOVA** (parametric) — used when each group has ≥ 10 stations.
- **Kruskal–Wallis** (non-parametric) — used for small groups (< 10); no normality assumption.
`auto` mode selects the appropriate test automatically.

> Kruskal, W. H., & Wallis, W. A. (1952).
> Use of ranks in one-criterion variance analysis.
> *Journal of the American Statistical Association*, 47(260), 583–621.
> https://doi.org/10.1080/01621459.1952.10483441

> Zar, J. H. (2010).
> *Biostatistical Analysis* (5th ed.). Prentice Hall, Upper Saddle River, NJ.

---

### Temporal Trend
**What it tells you:** Whether a metric is improving, declining, or stable over time at
a given station (requires multiple surveys with different dates). Computed as a linear
regression of the metric against survey date (ordinal). Useful for documenting reef
recovery after bleaching or cyclone events.

---

### Depth Gradient
**What it tells you:** Whether a metric (e.g., live coral cover) changes systematically
with water depth across stations — a common pattern on Indo-Pacific reefs where shallow
stations face more thermal stress and physical disturbance. Computed as a linear regression
of the metric against station depth (m).

---

## Multivariate Community Analysis

### Bray–Curtis Dissimilarity
**What it tells you:** How different two stations' benthic communities are in terms of
composition. 0 = identical; 1 = no codes in common. Uses proportions (not raw counts) so
stations with different total point counts can be compared fairly. The starting point for
all multivariate analyses (PCoA, clustering, PERMANOVA, SIMPER).
By default, abiotic codes (Sand, Rubble, etc.) and tape/wand/shadow (TWS) are excluded
so substrate does not dominate the distance.

> Bray, J. R., & Curtis, J. T. (1957).
> An ordination of the upland forest communities of southern Wisconsin.
> *Ecological Monographs*, 27(4), 325–349.
> https://doi.org/10.2307/1942268

---

### Principal Coordinates Analysis (PCoA)
**What it tells you:** Places stations in 2D space so that distances between points
approximate their Bray–Curtis dissimilarities. Stations with similar benthic communities
cluster together. Axis 1 captures the most variation, Axis 2 the second most.
Used to visualise community patterns across a project without assuming any groups.

> Gower, J. C. (1966).
> Some distance properties of latent root and vector methods used in multivariate analysis.
> *Biometrika*, 53(3–4), 325–338.
> https://doi.org/10.1093/biomet/53.3-4.325

---

### Hierarchical Clustering — UPGMA
**What it tells you:** Groups stations into a tree (dendrogram) based on their Bray–Curtis
similarity. Stations that merge at low dissimilarity are very similar in composition.
UPGMA (Unweighted Pair Group Method with Arithmetic mean) is the standard linkage
method in community ecology.

> Sokal, R. R., & Michener, C. D. (1958).
> A statistical method for evaluating systematic relationships.
> *University of Kansas Science Bulletin*, 38, 1409–1438.

---

### PERMANOVA
**What it tells you:** Tests whether the benthic community composition differs
significantly between pre-defined groups of stations (e.g., inside vs. outside a
marine protected area, different reef zones). Returns a pseudo-F statistic and a
p-value derived from permuting group labels — no multivariate normality assumed.
Requires ≥ 4 stations total and ≥ 2 per group.

> Anderson, M. J. (2001).
> A new method for non-parametric multivariate analysis of variance.
> *Austral Ecology*, 26(1), 32–46.
> https://doi.org/10.1111/j.1442-9993.2001.01070.pp.x

> McArdle, B. H., & Anderson, M. J. (2001).
> Fitting multivariate models to community data: a comment on distance-based
> redundancy analysis. *Ecology*, 82(1), 290–297.
> https://doi.org/10.1890/0012-9658(2001)082[0290:FMMTCD]2.0.CO;2

---

### SIMPER (Similarity Percentages)
**What it tells you:** Which specific benthic codes are responsible for the difference
between two groups of stations. Returns each code's average contribution (%) to the
mean Bray–Curtis dissimilarity between the groups. Answers the question: "Groups A and B
are different — but *what exactly* makes them different?"

> Clarke, K. R. (1993).
> Non-parametric multivariate analyses of changes in community structure.
> *Australian Journal of Ecology*, 18(1), 117–143.
> https://doi.org/10.1111/j.1442-9993.1993.tb00438.x

---

## Scientific Python Stack

> Harris, C. R., et al. (2020). Array programming with NumPy.
> *Nature*, 585, 357–362.
> https://doi.org/10.1038/s41586-020-2649-2

> Virtanen, P., et al. (2020). SciPy 1.0: Fundamental algorithms for scientific computing in Python.
> *Nature Methods*, 17, 261–272.
> https://doi.org/10.1038/s41592-019-0686-2

> McKinney, W. (2010). Data structures for statistical computing in Python.
> *Proceedings of the 9th Python in Science Conference*, 56–61.
> https://doi.org/10.25080/Majora-92bf1922-00a

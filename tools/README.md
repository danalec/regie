# Tools

The offline tools of the project. Every one of them uses the Python standard
library only: no numpy, no pandas, no network access.

| tool | what it does |
|---|---|
| [`train_intent.py`](train_intent.py) | trains the intent micro-model and writes the quantized weights into the crate |
| [`differential_test.py`](differential_test.py) | compares the engine against Python's `re` on tens of thousands of cases |
| [`check_examples.py`](check_examples.py) | runs every output that the documentation quotes and compares it with what is written down |
| [`add_serde_derives.py`](add_serde_derives.py) | adds the serde derives to the specification types |

## The trainer

```
python tools/train_intent.py
```

The script is fully deterministic: the same dataset always produces the same
`crates/regie-intent/src/weights.rs`, byte for byte.

### What it does

1. Reads `datasets/intent_train.jsonl`.
2. Splits it into a training part and a held-out part.  The split is
   stratified per class and follows the order of the file, so every sentence
   shape appears on both sides.
3. Extracts the lexical features of every request: the tokens, the pairs of
   adjacent tokens and a few coarse shape buckets.
4. Fits a Bernoulli Naive Bayes classifier over those features.  The fit is
   closed-form: the weight of a feature for a class is the log-odds ratio
   `log(P(feature | class) / P(feature | not class))`, and the bias of a class
   is `log(P(class))`.  There is no iteration, no seed and no shuffling, so
   the result cannot depend on the iteration order.
5. Quantizes the weights to `i8` with a single shared scale.  The scale
   preserves the order of the scores, so the argmax and the margin
   comparison stay exact integer operations.
6. Calibrates the two decision thresholds - the minimum score of the winner
   and the minimum gap to the runner-up - with a stratified cross-validation,
   so they hold for requests the weights were not fitted on.
7. Reports the accuracy on the held-out split and writes
   `crates/regie-intent/src/weights.rs`.

### The report

```
regie-intent training report
  examples            : 744
  classes             : 43
  train / eval split  : 602 / 142
  vocabulary          : 1419 features
  quantization        : i8, 127 levels, step 6.829721e-02
  margin threshold    : 52
  minimum score       : 12
  weight table        : 61189 bytes
  linked footprint    : 99121 bytes
  held-out accuracy   : 1.0000 (142/142)
  held-out resolved   : 0.9577 (136/142)
```

`weight table` counts the `i8` weights and the biases.  `linked footprint`
adds the vocabulary and the class names, with the fat pointers of the `&str`
arrays included, because they are linked just like the strings: the whole
model occupies about 97 KiB, well below the 256 KiB budget of the project.

`held-out accuracy` is the share of held-out requests whose best class is
the right one.  `held-out resolved` is the share that is also confident
enough to be accepted; the rest are refused as ambiguous, which is the
intended behaviour for a request the model is not sure about.

The corpus is generated from templates, so these numbers say very little
about real requests.

### Regenerating the dataset

The dataset is generated from the corpus table at the top of the script:

```
python tools/train_intent.py --emit-dataset
```

### Keeping the two implementations in sync

The feature extractor in `tools/train_intent.py` and the one in
`crates/regie-intent/src/features.rs` are mirrors of each other.  They have
to agree exactly, otherwise the model would be scored with features it was
never trained on.  When one of them changes, change the other in the same
commit and re-run the trainer.

## The differential test

```
python tools/differential_test.py --cases 20000
```

The harness runs the same patterns through the engine and through Python's
`re`, in both directions: every case where the engine matches has to match in
Python as well, and every case where the engine refuses the pattern has to be
refused there too.  The current result is zero mismatches over 21,980 cases.

## The example checker

```
python tools/check_examples.py
```

The galleries quote the output of the programs they document.  A quoted output
goes stale the moment the code changes, and a stale quote is worse than no
quote, because a reader trusts it.

This tool reads every Markdown file that quotes an output, runs the command,
and compares what it observed with what the file claims.  A mismatch is a
failure, and the continuous integration runs it on every build.

The commands that write a file run in a temporary directory, so the check
never leaves anything behind in the repository.

## The serde helper

`add_serde_derives.py` adds the `Serialize` and `Deserialize` derives to the
specification types of `regie-core`.  It is a one-off maintenance script: run
it when a new type has to be serializable, and check the diff before
committing it.

# Intent tooling

`train_intent.py` trains the intent micro-model of `regie-intent` and writes
the quantized weights into the crate.

```
python tools/train_intent.py
```

The script uses the standard library only - no numpy, no pandas, no network
access - and is fully deterministic: the same dataset always produces the
same `crates/regie-intent/src/weights.rs`, byte for byte.

## What it does

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

## The report

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

## Regenerating the dataset

The dataset is generated from the corpus table at the top of the script:

```
python tools/train_intent.py --emit-dataset
```

## Keeping the two implementations in sync

The feature extractor in `tools/train_intent.py` and the one in
`crates/regie-intent/src/features.rs` are mirrors of each other.  They have
to agree exactly, otherwise the model would be scored with features it was
never trained on.  When one of them changes, change the other in the same
commit and re-run the trainer.

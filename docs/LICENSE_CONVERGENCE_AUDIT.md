# ExtremEngine license-convergence audit

Status: **audit record only — no relicensing performed by this document**.

This document records the current repository licensing surfaces that must be reconciled before ExtremEngine can be aligned with the Memorithm/SciRust PolyForm Noncommercial regime. It is an engineering/provenance record, not a legal determination.

## Source of truth requested for Memorithm first-party code

The organization-level convergence target is the regime actually present on the default branch of `Memorithm/scirust`:

- PolyForm Noncommercial License 1.0.0;
- SPDX identifier `PolyForm-Noncommercial-1.0.0`;
- Required Notice naming Tarek Zekriti;
- commercial use not granted by the noncommercial license;
- a separate written commercial agreement may be offered by the copyright holder.

The complete license text, third-party notices, provenance, and any non-waivable rights remain authoritative over summaries.

## ExtremEngine state observed on the authoritative trunk

Authoritative trunk: `agent/initial-engine`.

At the time of this audit:

- workspace `Cargo.toml` declares `license = "MIT OR Apache-2.0"`;
- the repository contains `LICENSE-MIT` with `Copyright (c) 2026 Memorithm` and an unrestricted MIT grant, including commercial use and sublicensing;
- no root `LICENSE-APACHE` file was present in the root listing/search performed for this audit;
- the repository does not currently expose the SciRust-style root `LICENSE`, `LICENSE.md`, and `LICENSING.md` PolyForm set.

These facts mean the repository is **not converged** with SciRust today.

## Why this audit does not rewrite the license

An existing permissive license grant must not be silently deleted, rewritten, or described as if rights already granted to recipients had never existed. In addition, changing the license of historical material requires knowing which copyrights and relicensing permissions are actually controlled by the proposed licensor.

Repository history, vendored/generated material, copied examples/assets, and external contributions therefore need to be classified before any automated license replacement. A Git author identity or organization ownership of a repository is not by itself proof that every copyright in every historical file can be relicensed.

Until that provenance classification is complete, automation must **not**:

- delete or overwrite `LICENSE-MIT`;
- represent previously MIT-licensed copies as retroactively noncommercial;
- add a PolyForm copyright notice to third-party material whose copyright is not established;
- replace third-party notices or dependency licenses;
- change package metadata to PolyForm while leaving incompatible or ambiguous repository-level licensing text unresolved.

## Required provenance classification before a transition PR

A later dedicated transition increment should classify, at minimum:

1. all current first-party source files and their introducing commits;
2. commit authors/copyright contributors relevant to those files;
3. vendored, generated, copied, sample, shader, asset, specification, and test-vector material;
4. third-party notices and files carrying their own SPDX or license headers;
5. whether any Apache-2.0 grant represented by current Cargo metadata has corresponding license/provenance material elsewhere in history;
6. whether any published release/tag/crate was already distributed under MIT or `MIT OR Apache-2.0`.

The classification must preserve historical provenance even if all current contributors ultimately authorize a future-license change.

## Safe transition shape if rights are established

If the provenance audit establishes that the relevant current first-party work can be relicensed, the transition should be a separate reviewed PR and should distinguish clearly between:

- the license governing the new/current first-party version from the declared transition commit onward;
- historical permissive grants that remain valid for copies obtained under those terms;
- third-party/vendored components that continue under their own licenses.

The transition may then add the SciRust-style PolyForm files and package metadata where legally applicable, but it must retain any notices or historical-license documentation necessary to avoid misrepresenting earlier grants.

## Current blocker

**Blocked pending provenance/rightsholder classification.**

No conclusion is recorded here that Tarek Zekriti, Memorithm, or any other party owns every copyright required to relicense the full ExtremEngine history. The audit intentionally stops before making that claim.

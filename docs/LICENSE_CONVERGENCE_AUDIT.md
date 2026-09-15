# ExtremEngine license-convergence audit

Status: **audit record only — no relicensing performed by this document**.

This document records the current repository licensing surfaces that must be reconciled before ExtremEngine can be aligned with the Memorithm/SciRust PolyForm Noncommercial regime. It is an engineering/provenance record, not a legal determination.

## Pinned audit provenance

Audit date: **2026-09-15**.

The licensing facts below were inspected against these exact repository revisions:

- SciRust source-of-truth revision: `Memorithm/scirust@4e92a89b94e69d3c87571a9470c2b03da2ea5fd8` on its `master` default branch;
- ExtremEngine audited trunk revision: `Memorithm/ExtremEngine@01ca5f482cad7a8d4543fb4c6a333ff7eae5c492` on its `agent/initial-engine` default branch.

A later transition must refresh this audit if either repository has materially changed its licensing/provenance surfaces. Mutable branch names are context only; the SHAs above identify the evidence used by this record.

## Source of truth requested for Memorithm first-party code

At the pinned SciRust revision above, the convergence target is:

- PolyForm Noncommercial License 1.0.0;
- SPDX identifier `PolyForm-Noncommercial-1.0.0`;
- Required Notice naming Tarek Zekriti;
- commercial use not granted by the noncommercial license;
- a separate written commercial agreement may be offered by the copyright holder.

The complete license text, third-party notices, provenance, and any non-waivable rights remain authoritative over summaries.

## ExtremEngine state observed on the authoritative trunk

Authoritative trunk at audit time: `agent/initial-engine`, pinned above.

At the pinned ExtremEngine revision:

- workspace `Cargo.toml` declares `license = "MIT OR Apache-2.0"`;
- the repository contains `LICENSE-MIT` with `Copyright (c) 2026 Memorithm` and an unrestricted MIT grant, including commercial use and sublicensing;
- no root `LICENSE-APACHE` file was present in the root listing/search performed for this audit;
- the repository does not currently expose the SciRust-style root `LICENSE`, `LICENSE.md`, and `LICENSING.md` PolyForm set.

These facts mean the audited revision is **not converged** with the pinned SciRust licensing regime.

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

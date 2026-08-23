# Code signing and notarization: requirements and costs (macOS, Windows)

This document answers one question — **what do macOS notarization and Windows code signing actually require of us, and what do they cost?** — for Tiller's planned release on Windows, Linux and macOS with an in-app auto-updater. It is a record of *facts with sources*, not a recommendation: the choice of which certificate, which CA, and whether to sign at all is a separate decision for a human to make against these facts. Every figure below is dated and attributed to the page it was taken from. Facts were gathered on **2026-08-23**; prices and CA/Browser Forum rules both change, and several of the numbers here have a stated effective date in 2026, so re-verify before committing money.

## Source discipline used here

Only primary sources: Apple Developer documentation and Apple Support, Microsoft Learn and Microsoft's own pricing API, the CA/Browser Forum Code Signing Baseline Requirements, and certificate authorities' own pricing pages. No blog posts, no Stack Overflow, no reseller pages quoted as authoritative. Where a price is not publicly published — which is the norm for EV — that is stated as a gap rather than filled in from a reseller. Where two primary sources disagree, both are quoted and the conflict is flagged.

Two mechanical notes. Quotations from the CA/Browser Forum PDF have had inter-word spaces restored: the PDF's text layer extracts without spaces (`CAsSHALLensure…`), so the words are Apple-to-apple identical but the whitespace is reconstructed. Apple's developer documentation pages are client-rendered; the text quoted here was read from the JSON documents that back them (`developer.apple.com/tutorials/data/documentation/...json`), which is the same content the human-facing page renders.

---

# Part 1 — macOS

## 1.1 Apple Developer Program: cost and enrolment

The fee is the same whether you enrol as an individual or as an organization.

> "The Apple Developer Program annual fee is 99 USD and the Apple Developer Enterprise Program annual fee is 299 USD, in local currency where available. Prices may vary by region and are listed in local currency during the enrollment process."
> — <https://developer.apple.com/support/enrollment/> (read 2026-08-23)

The Apple Developer Program page states the same figure as "$99 annual membership" (<https://developer.apple.com/programs/>). The **Enterprise Program at 299 USD is not the relevant product** for Tiller — it exists for in-house distribution to employees, not public distribution — so the number to plan against is **99 USD/year**.

**Individual / sole proprietor enrolment** requires the person's legal name; Apple's enrolment page notes that a sole proprietor or single-person business enrols as an individual, and that the personal legal name is what gets listed as the seller. No D-U-N-S number is involved.

**Organization enrolment** is materially heavier. From the same page:

> "To enroll in the Apple Developer Program, your organization must be a legal entity so that it can enter into contracts with Apple. We don't accept DBAs, fictitious businesses, trade names, or branches."

> "Your organization must have a D-U-N-S Number so that we can verify your organization's identity and legal entity status. These unique nine-digit numbers are assigned by Dun & Bradstreet and are widely used as standard business identifiers."

> "You must be the organization's owner/founder, executive team member, senior project lead, or an employee with legal authority granted to you by a senior employee."

Organizations additionally need a work email on the organization's own domain and a publicly available, functional website on that domain. D-U-N-S numbers are free in most jurisdictions per the same page. **Practical consequence for a solo maintainer: individual enrolment costs the same 99 USD and skips the D-U-N-S / legal-entity / corporate-website gate entirely.** The trade-off is that the developer's personal legal name, not a company name, appears as the signing identity.

## 1.2 What a Developer ID Application certificate covers

Apple's certificate reference (<https://developer.apple.com/support/certificates/>) lists 20 certificate types. Two matter here:

| Certificate | Purpose (Apple's wording) |
| --- | --- |
| **Developer ID Application** | "Sign Mac apps for outside Mac App Store distribution" |
| **Mac App Distribution** | "Sign Mac apps for Mac App Store" |

They are not interchangeable, and this is enforced at the notary service, not merely by convention. Apple's notarization requirements state:

> "Use a 'Developer ID' application, kernel extension, system extension, or installer certificate for your code-signing signature. (Don't use a Mac Distribution, ad hoc, Apple Developer, or local development certificate.)"
> — <https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution>

There is also a **Developer ID Installer** certificate for signing `.pkg` installer packages distributed outside the store. If Tiller ships a `.pkg` (as opposed to a `.dmg` containing a `.app`), both certificate types are needed.

Two operational facts from Apple's certificates page:

- Developer ID certificates are the documented **exception to the one-per-team rule**: "Distribution certificates belong to the team and only one type of each distribution certificate (with the exception of Developer ID certificates) is allowed per team."
- Apple's Account Help puts a hard cap on that exception: **up to five Developer ID Application certificates and up to five Developer ID Installer certificates**, and only the **Account Holder** role can create them (<https://developer.apple.com/help/account/create-certificates/create-developer-id-certificates>). For a one-person team that is a non-issue; for a team it means the person who can mint the CI signing identity is a single named individual.

## 1.3 Notarization and stapling from CI

### What notarization is, in Apple's words

> "Notarize your macOS software to give users more confidence that the Developer ID-signed software you distribute has been checked by Apple for malicious components. Notarization of macOS software is not App Review. The Apple notary service is an automated system that scans your software for malicious content, checks for code-signing issues, and returns the results to you quickly. If there are no issues, the notary service generates a ticket for you to staple to your software; the notary service also publishes that ticket online where Gatekeeper can find it."
> — <https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution>

### The seven preconditions Apple enforces

From the same page, verbatim:

1. "Enable code-signing for all of the executables you distribute, and ensure that executables have valid code signatures"
2. "Use a 'Developer ID' application, kernel extension, system extension, or installer certificate for your code-signing signature. (Don't use a Mac Distribution, ad hoc, Apple Developer, or local development certificate.)"
3. "Enable the Hardened Runtime capability for your app and command line targets"
4. "Include a secure timestamp with your code-signing signature"
5. "Don't include the `com.apple.security.get-task-allow` entitlement with the value set to any variation of `true`"
6. "Link against the macOS 10.9 or later SDK"
7. "Ensure your processes have properly-formatted XML, ASCII-encoded entitlements"

Items 3 and 5 are the ones that bite an application shipping a terminal emulator and spawning child processes: the Hardened Runtime restricts things like JIT, unsigned executable memory, and library loading unless the corresponding entitlement is declared. That is a code change, not a paperwork change, and it should be budgeted as engineering work rather than as part of the certificate purchase.

### The `notarytool` flow

`altool` is dead for this purpose. Apple's TN3147 states that Apple "announced that it will stop working for notarization on 2023-11-01" (<https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool>). `notarytool` ships with Xcode 13 and later.

Submission, with `--wait` so CI blocks rather than polls:

```sh
% xcrun notarytool submit OvernightTextEditor_11.6.8.zip \
                   --keychain-profile "notarytool-password" \
                   --wait \
                   --webhook "https://example.com/notarization"
```

Success returns a submission id and a status:

```
createdDate: 2021-04-29T01:38:09.498Z
id: 2efe2717-52ef-43a5-96dc-0797e4ca1041
name: OvernightTextEditor_11.6.8.zip
status: Accepted
```

On rejection, the JSON log explains why:

```sh
% xcrun notarytool log 2efe2717-52ef-43a5-96dc-0797e4ca1041 \
         --keychain-profile "notarytool-password" developer_log.json
```

All the above from <https://developer.apple.com/documentation/security/customizing-the-notarization-workflow>.

### Credentials: app-specific password vs App Store Connect API key

`notarytool` accepts three credential forms. Apple documents the first two on the notarization workflow page and all three in TN3147.

**(a) Apple ID + app-specific password.** Required because App Store Connect enforces 2FA on all accounts:

```sh
% xcrun notarytool submit OvernightTextEditor_11.6.8.zip \
                   --apple-id "<AppleID>" \
                   --password <secret_2FA_password> \
                   --team-id <DeveloperTeamID> \
                   --wait
```

App-specific passwords are generated at <https://account.apple.com> under Sign-In and Security → App-Specific Passwords. Apple caps them at **25 active passwords**, they can be revoked individually or all at once, and critically: *"Any time you change or reset your primary Apple Account password, all of your app-specific passwords are revoked automatically."* (<https://support.apple.com/en-us/102654>, published 2025-10-08). That last clause means a routine Apple Account password rotation silently breaks the release pipeline.

**(b) App Store Connect API key.** TN3147 gives the flags:

```sh
% xcrun notarytool … --issuer ISSUER_UUID --key-id API_KEY --key PATH_TO_KEY …
```

where `ISSUER_UUID` is the App Store Connect API key issuer UUID (e.g. `c055ca8c-e5a8-4836-b61d-aa5794eeb3f4`), `API_KEY` is the 10-character key ID (e.g. `T9GPZ92M7K`), and `PATH_TO_KEY` is the path to the `.p8` file. TN3147 notes: *"There's no need to supply your Apple ID because `notarytool` works that out based on the issuer."*

**There is a trap here that decides the choice.** Apple's API key documentation distinguishes two key types:

> "There are two types of API keys:
> - Team: Access to all apps, with varying levels of access based on selected roles.
> - Individual: Access and roles of the associated user. Individual keys aren't able to use Provisioning endpoints, access Sales and Finance, or notaryTool."
> — <https://developer.apple.com/documentation/appstoreconnectapi/creating-api-keys-for-app-store-connect-api>

**An Individual API key cannot notarize.** A Team key is required. The same page warns that the private key is **downloadable exactly once**: "The private key is available for download a single time" and "Apple doesn't keep a copy of the private key", with the instruction "Don't share your keys, store keys in a code repository, or include keys in client-side code. If the key becomes lost or compromised, remember to revoke it immediately."

**(c) Keychain profile.** For interactive/self-hosted use, credentials can be stashed in the login keychain:

```sh
% xcrun notarytool store-credentials "notarytool-password" \
               --apple-id "<AppleID>" \
               --team-id <DeveloperTeamID> \
               --password <secret_2FA_password>
```

and then referenced with `--keychain-profile "notarytool-password"`. This is convenient on a developer machine but is *not* a way to avoid holding a secret on ephemeral CI runners — the profile has to be created from the underlying credential on every fresh runner, so the underlying credential is still what CI holds.

### Stapling

```sh
% xcrun stapler staple "Overnight TextEditor.app"
```

Apple's notes on the limits of stapling, from the notarization workflow page:

- ZIP archives can be notarized but **cannot be stapled to directly**. The documented workaround is to run `stapler` against each item that goes into the archive, then rebuild the ZIP from the stapled items.
- Tickets are created for standalone binaries but **cannot currently be stapled to them**.
- Stapling is what makes the notarization survive an offline first launch: users can fetch the ticket online, but "stapling ensures functionality without network connectivity."

For a `.dmg`-based distribution the practical order is: sign the `.app` → notarize → staple the `.app` → build the `.dmg` → notarize the `.dmg` → staple the `.dmg`.

### CI network egress

The same page lists hosts a build server needs to reach:

- S3 Transfer Acceleration (default): `notary-submissions-prod.s3-accelerate.amazonaws.com`
- Alternative with `--no-s3-acceleration`: `notary-submissions-prod.s3.us-west-2.amazonaws.com`
- CloudKit ticket downloads on port 443, required by `stapler`.

## 1.4 What a user actually sees at first launch

This is the part where the version of macOS matters, because Apple removed the old escape hatch.

> "In macOS Sequoia, users will no longer be able to Control-click to override Gatekeeper when opening software that isn't signed correctly or notarized. They'll need to visit System Settings > Privacy & Security to review security information for software before allowing it to run."
> — Apple Developer news, 2024-08-06, <https://developer.apple.com/news/?id=saqachfa>

So any instruction of the form "right-click → Open" is obsolete for macOS 15 and later. Apple's current end-user article is "Safely open apps on your Mac" (<https://support.apple.com/en-us/102445>, published 2026-05-27), and its illustrations are all `macos/sequoia/` assets.

### Case A — signed with Developer ID **and** notarized

> "The first time that you open a new app from an identified developer that you downloaded outside the App Store, your Mac asks if you're sure that you want to open it."
> — <https://support.apple.com/en-us/102445>

Apple's platform security guide gives the reason this consent prompt exists at all even for a perfectly notarized app:

> "Gatekeeper also requests user approval before opening downloaded software for the first time to make sure the user hasn't been tricked into running executable code they believed to simply be a data file."
> — <https://support.apple.com/guide/security/gatekeeper-and-runtime-protection-sec5599b66df/web>

And the notarization ticket is what upgrades that prompt from a warning to a confirmation:

> "When the user first installs or runs your macOS software, the presence of a ticket (either online or attached to the executable) tells Gatekeeper that Apple notarized the software. Gatekeeper then places descriptive information in the initial launch dialog to help the user make an informed choice about whether to launch the app."
> — <https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution>

**Net user experience: one confirmation dialog naming the verified developer, with a button that opens the app.** No trip to System Settings, no dead end. This is the only case where a non-technical user gets the app running without being told to do something that looks like disabling a security feature.

### Case B — signed with Developer ID but **not** notarized

Apple's article covers this under "Alert that Apple cannot check the app for malicious software". The alert image on that page carries this alt text, which is the closest Apple publishes to the dialog's literal wording:

> Alert message stating "Apple cannot check 'Example App' for malicious software" with options to "Move to Trash" or "Done".

The asset filename backing it is `macos-sequoia-app-not-opened-could-not-verify-free-from-malware.png`, i.e. the dialog is the *"… Not Opened"* / *could not verify free from malware* variant. **The two buttons offered are "Move to Trash" and "Done". Neither of them opens the app.** The user is at a dead end in the dialog itself and must know to go elsewhere.

Apple's own advice to the user in this state is, in order: "Contact the app developer for more information", "Check the App Store for an updated version or search for an alternative app", and only then the override.

The override, verbatim from the same article:

> "After you've tried to open the app, follow these steps:
> 1. Open System Settings.
> 2. Click Privacy & Security, scroll down, and click the Open Anyway button to confirm your intent to open or install the app.
> 3. The warning prompt reappears and, if you're absolutely sure that you want to open the app anyway, you can click Open.
> The app is now saved as an exception to your security settings, and you can open it in the future by double-clicking it, just as you can any authorized app."

The macOS User Guide version of the same procedure (<https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac>) adds two details that matter for support load: the **"Open Anyway" button "is available for about an hour after you try to open the app"**, and the user must "Enter your login password, then click OK". So the workaround is *time-boxed* and requires *admin authentication*. A user who reads the instructions an hour later has to start over by launching the app again first.

**Net user experience: a scary two-button dialog with no "open" option, followed by a five-step, password-gated, one-hour-windowed detour through System Settings.** Signing without notarizing buys the developer's verified name in the alert and essentially nothing else in the launch path.

### Case C — neither signed nor notarized

Apple's article covers this under "Alert that the app developer cannot be verified":

> "If the app developer can't be verified and — in macOS Catalina and later — the app hasn't been notarized by Apple, macOS can't verify that the app is free of malware."

The remedy offered is identical to Case B — the same System Settings → Privacy & Security → Open Anyway detour. **From the user's point of view, unsigned-and-unnotarized and signed-but-unnotarized are close to the same experience**; the difference is that in Case B the alert can name a verified developer and in Case C it cannot. Apple frames the risk plainly on the same page: "Running software that hasn't been signed and notarized may expose your computer and personal information to malware that can harm your Mac or compromise your privacy."

There is a fourth state worth knowing about because it is the one that is *not* recoverable: if Gatekeeper concludes the software is malicious or its authorization was revoked, the article says "your Mac notifies you that the app will damage your computer", and for known malware macOS "notifies you that the app can't be opened and moves it to the Trash." No override.

Finally, a distinct failure mode that has nothing to do with our signing choices: if the user's Privacy & Security setting is "App Store" only, "macOS won't open an app that wasn't downloaded from the App Store" regardless of Developer ID or notarization. The two selectable options are "App Store" and "App Store and identified developers", and Apple notes "These settings might not be available if your Mac is managed by a system administrator or IT department."

---

# Part 2 — Windows

## 2.1 OV versus EV: what each requires of the applicant

The governing document is the CA/Browser Forum **Code Signing Baseline Requirements**, currently **v3.11.0**, adopted by ballot CSCWG-32 (2026-06-16). Index: <https://cabforum.org/working-groups/code-signing/documents/>. PDF: <https://cabforum.org/uploads/CA-Browser-Forum-CSCBR-3.11.0.pdf>. The CSBR calls the OV tier "Non-EV Code Signing Certificates".

### The eligibility difference that decides this for a solo maintainer

**EV cannot be issued to a private individual.** CSBR §4.1.1:

> "For EV Code Signing Certificates, the CA MAY only issue to Applicants that meet the Private Organization, Government Entity, Business Entity and Non-Commercial Entity requirements specified below."

A sole trader can potentially qualify as a **Business Entity** under §4.1.1.3, but only if "The entity is a legally recognized entity that filed certain forms with a Registration Agency in its jurisdiction, the Registration Agency issued or approved the entity's charter, certificate, or license, and the entity's existence can be verified with that Registration Agency", plus "The entity has a verifiable physical existence and business presence" and at least one Principal Individual identified and validated by the CA. That is a registered business, not a person with a GitHub account. (SSL.com sells an explicit "EV Sole Proprietor" product for exactly this case — see pricing below.)

**OV can be issued to an individual.** CSBR §3.2.3.1 sets out how:

> "The CA MUST obtain a legible copy, which discernibly shows the Certificate Requester's face, of at least one currently valid government-issued photo ID (passport, driver's license, military ID, national ID, or equivalent document type). The CA MUST inspect the copy for any indication of alteration or falsification."

plus address verification via government photo ID, a QIIS/QGIS, or a physically-mailed activation code. §3.2.3.2 then requires the CA to confirm the request is genuinely from that person, by one of: a photo of the requester holding the ID, an in-person or webcam verification, an executed Declaration of Identity with a biometric identifier, or a qualified digital signature.

### What OV requires of an *organizational* applicant

CSBR §3.2.2.1 — verify legal identity and any DBA, verify address, verify the requester's authority via a Reliable Method of Communication, and:

> "If the Subject's or Subject's Affiliate's, Parent Company's, or Subsidiary Company's date of formation, as indicated by either a QIIS or QGIS, was less than three years prior to the date of the Certificate Request, verify the identity of the Certificate Requester."

Note the shape of that three-year rule: for OV, an organization younger than three years does not become ineligible — it triggers an *additional individual identity check* on the requester.

### What EV requires beyond OV

CSBR §3.2.2.2 requires the CA to verify three separate things about the applicant, not one:

> "1. Verify Applicant's existence and identity, including;
>   1. Verify the Applicant's legal existence and identity …,
>   2. Verify the Applicant's physical existence (business presence at a physical address), and
>   3. Verify the Applicant's operational existence (business activity)."

and to verify authorization through **three named human roles** — Certificate Requester, Certificate Approver, and Contract Signer — each of whose "name, title, and authority" the CA must verify. One person may hold all three roles, but the roles have to exist and be attested. §3.2.2.2.1 further requires the CA to confirm the applicant is "not designated on the records of the Incorporating or Registration Agency by labels such as 'inactive', 'invalid', 'not current', or the equivalent."

**Summary of the applicant burden:** OV = prove who you are (individual or entity). EV = prove your *entity* legally exists, physically exists, and is *operating*, and route the request through named, verified officers of that entity.

### Certificate validity — this changed in 2026 and it changes the cost model

CSBR §6.3.2:

> "For Code Signing Certificates issued before March 1st, 2026, the validity period MUST NOT exceed 39 months. For Code Signing Certificates issued on or after March 1st, 2026, the validity period MUST NOT exceed 460 days."

Two CAs confirm this from their own side. GlobalSign's code signing page states "Only 1-year (366-day) Code Signing Certificates are now available" as of 2025-12-26, with certificates reaching up to 460 days on renewal (<https://www.globalsign.com/en/code-signing-certificate>). Certum's shop notes on every code signing product: "Starting from **February 27, 2026**, a single Code Signing certificate may be valid for a maximum of **459 days**. Therefore, when purchasing a 2 or 3 year product, one or more free reissues will be required during the service period." (<https://shop.certum.eu/code-signing.html>).

**Practical consequence: the "buy a 3-year cert and forget about it" option no longer exists.** Multi-year products are now subscriptions that force a reissue — and therefore a re-key and a CI secret rotation — at least annually.

### Published prices

Prices as displayed on each CA's own page, read 2026-08-23. Currency is as shown on the page; where the page shows a bare `$` on a US site it is treated as USD and flagged.

**DigiCert** — <https://www.digicert.com/signing/code-signing-certificates>

| Product | Displayed rate | Displayed subscription |
| --- | --- | --- |
| OV Code Signing | "Starting at $44 / month / certificate", "12 month auto-renewing" | "subscription $696.00" |
| EV Code Signing | "$62 / month / certificate", "12 month auto-renewing" | "subscription $972.00" |

⚠️ These two figures do not reconcile arithmetically: $44 × 12 = $528, not $696; $62 × 12 = $744, not $972. The most likely reading is that the "starting at" monthly rate is the floor across longer terms while the displayed subscription is the 12-month price, but **the page does not say so** and I did not confirm it. Treat **$696/year OV and $972/year EV** as the concrete annual figures and the monthly rates as unexplained. The page carries "Prices subject to change." DigiCert lists four key-storage variants (KeyLocker cloud, DigiCert-provided hardware token, your own qualified token, your own HSM) but **publishes no separate price for any of them**.

**SSL.com** — product pages read 2026-08-23

| Product | 1 year | 5 year rate | URL |
| --- | --- | --- | --- |
| IV Code Signing (individual) | **$129.00/yr** | $96.75/yr | <https://www.ssl.com/products/software-integrity/code-signing/iv/> |
| OV Code Signing | **$129.00/yr** | $96.75/yr | <https://www.ssl.com/products/software-integrity/code-signing/ov/> |
| EV Code Signing | **$349.00/yr** | $149.00/yr | <https://www.ssl.com/products/software-integrity/code-signing/ev/> |
| EV Sole Proprietor | **$359.00/yr** | — | listed on the eSigner page |

Key storage is priced separately and is not optional in substance (see §2.2):

- **eSigner cloud signing** (SSL.com's cloud HSM): Tier 1 **$15.00/mo** for 240 signings and 1 credential; Tier 2 $63.75/mo (1,200 signings, 5 credentials); Tier 3 $131.25/mo (3,600); Tier 4 $187.50/mo (12,000). Extra credentials $20/month. "New certificates will receive 30-days free of eSigner cloud signing." Annual billing "save over 25% monthly". — <https://www.ssl.com/esigner/>
- **YubiKey physical token: +$379.00 each** on the certificate product pages; the eSigner page shows **+$249.00 each** for the same item. ⚠️ Two SSL.com pages disagree on the token price; I could not determine which is current.
- **Bring your own cloud HSM** attracts a one-time attestation fee: AWS CloudHSM **$1,500**, Google Cloud HSM **$500** (the eSigner page shows $1,500 for Google — ⚠️ a second internal disagreement), Azure Dedicated HSM **$500.00**.
- Expedited validation +$599.00.

SSL.com also warns on its EV page that "YubiKey tokens are fully suitable for OV code signing. If you require EV code signing, particularly for kernel-mode driver signing (Microsoft HLK), a YubiKey may not meet those requirements." Not relevant to Tiller (no kernel driver) but relevant to anyone reusing this note.

**Certum** — <https://shop.certum.eu/code-signing.html>, prices in EUR, "gross" and "net" shown as equal on the listing

| Product | From |
| --- | --- |
| Open Source Code Signing in the Cloud | **€49.00** |
| Standard Code Signing in the Cloud | **€209.00** |
| EV Code Signing in the Cloud | **€379.00** |
| Standard Code Signing — set (cert + cryptoCertum card) | **€169.00** |
| EV Code Signing — set (cert + cryptoCertum card) | **€359.00** |

Certum's cloud products use its SimplySign service, which the shop describes as eliminating "the need to use a physical card and reader". The **Open Source** product at €49.00 is explicitly aimed at "a programmer or developer" who shares software "as free to use" / "as open source project" — this is the cheapest publicly-priced route to a real OV certificate found in this survey. I did **not** verify Certum's eligibility criteria for that product (what evidence of open-source status it demands, or whether it is issued to an individual or requires an entity); that needs checking before relying on it.

**GlobalSign** — <https://www.globalsign.com/en/code-signing-certificate>. **No prices published.** The page renders "From / year" with no amount for both OV and EV and routes to a purchase flow or sales contact. It does state that "GlobalSign Code Signing Certificates are automatically shipped with a standards-compliant cryptographic USB token" and that an "HSM implementation option is available if selected at checkout", with Azure Key Vault HSM compatibility.

**On EV pricing generally:** DigiCert and SSL.com publish EV list prices; GlobalSign does not. Where a CA does not publish, this document says so rather than quoting a reseller. Actual EV cost also depends on the token/HSM line item, which DigiCert does not publish at all.

## 2.2 The June 2023 hardware key requirement — confirmed

This is real, it is in the baseline requirements, and it applies to **OV as well as EV**. CSBR v3.11.0 §6.2.7.4.1 (spaces restored from the PDF text layer):

> "Effective June 1, 2023, Subscriber Private Keys for Code Signing Certificates SHALL be protected per the following requirements. The CA MUST obtain a contractual representation from the Subscriber that the Subscriber will use one of the following options to generate and protect their Code Signing Certificate Private Keys in a Hardware Crypto Module with a unit design form factor certified as conforming to at least FIPS 140-2 Level 2 or Common Criteria EAL 4+:
> 7. Subscriber uses a Hardware Crypto Module meeting the specified requirement;
> 8. Subscriber uses a cloud-base key generation and protection solution with the following requirements:
>    1. Key creation, storage, and usage of Private Key must remain within the security boundaries of the cloud solution's Hardware Crypto Module that conforms to the specified requirements;
>    2. Subscription at the level that manages the Private Key must be configured to log all access, operations, and configuration changes on the resources securing the Private Key.
> 9. Subscriber uses a Signing Service which meets the requirements of Section 6.2.7.3."

Before this date, §6.2.7.4.1 permitted OV subscribers to use a TPM, a certified module, **or** "another type of hardware storage token with a unit design form factor of SD Card or USB token (not necessarily certified as conformant with FIPS 140-2 Level 2 or Common Criteria EAL 4+)". The June 2023 change removed the uncertified option and levelled OV up to what EV already required.

The CSBR does not take the Subscriber's word for it. §6.2.7.4.2 requires the CA to *verify* the claim by one of seven methods, including shipping a pre-keyed module itself, key attestation ("The Subscriber counter-signs certificate requests that can be verified by using a manufacturer's certificate, commonly known as key attestation, indicating that the Private Key was generated in a non-exportable way using a suitable Hardware Crypto Module"), a prescribed crypto library + module combination, an IT audit, a cloud-configuration report, an auditor-witnessed key ceremony, or an agreement that the Subscriber uses a compliant Signing Service.

### What this means practically for CI signing

**A code signing private key cannot exist as a file. There is no `.pfx` in a GitHub secret.** That is the whole point of the rule, and it is the single largest change to how Windows signing has to be architected. Three shapes remain:

1. **Physical USB token.** The key never leaves the token, and the token has to be physically plugged into the signing machine. This is incompatible with hosted CI runners. It means either a self-hosted runner with the token attached, or a manual signing step in the release process. This is the option that most directly conflicts with an automated release pipeline.
2. **Bring-your-own cloud HSM** (AWS CloudHSM / Google Cloud HSM / Azure Dedicated HSM / Azure Key Vault Managed HSM, depending on CA support). CSBR-compliant and CI-drivable, but the CA has to attest the key — SSL.com charges $500–$1,500 one-time for that (see §2.1), and Azure Dedicated HSM in particular is an expensive standing resource.
3. **A Signing Service** under §6.2.7.3 — the CA or a third party holds the key in its HSM and you authenticate to an API. SSL.com's eSigner, DigiCert KeyLocker, and Microsoft's Artifact Signing are all of this shape. **This is the only shape where the CI secret is an API credential rather than a key, which changes the blast radius materially (see §3.1).**

Notably, the requirement is met and *verified* by the service in option 3 — Artifact Signing's docs state "Artifact Signing does *not* support importing or exporting private keys and certificates" (<https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management>), which is exactly the non-exportability §6.2.7.4.2(2) is after.

## 2.3 SmartScreen reputation — and the finding that overturns the folklore

Microsoft's own developer-facing page is <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation> (ms.date 2026-05-04, updated 2026-08-17).

### How reputation is computed

> "SmartScreen evaluates two signals when a user downloads and runs a file:
> 1. **Publisher reputation** — Is the file signed? Is the signing certificate from a known, trusted publisher?
> 2. **File hash reputation** — Has this specific file been downloaded by users without indications of malicious behavior?
>
> A negative or unknown reputation for a file's hash or its publisher's certificate can cause warnings to show. Even when signed, a newly created binary could still show a SmartScreen warning until its hash or publisher certificate accumulates sufficient evidence of positive reputation."

> "When a file is not signed, SmartScreen reputation must build for each new version of your files, starting with zero reputation. Reputation cannot transfer from previous versions unless both were signed using the same publisher identity."

So reputation is **two-track**: per-file-hash and per-signing-identity. Signing is what lets a *new* build inherit anything at all; without it every release starts from zero.

### Does EV grant immediate reputation? No — not any more

This is the headline. Microsoft states it flatly:

> "**EV certificates no longer bypass SmartScreen.** Years ago, signing files with an Extended Validation (EV) code signing certificate would result in positive SmartScreen reputation by default, but this behavior no longer exists. EV certificates may matter for enterprise procurement, but they no longer impact SmartScreen behavior. Paying a premium for EV solely to avoid SmartScreen warnings is no longer justified."

Microsoft's per-certificate-type table on the same page shows OV and EV in **the same row with the same behaviour**:

| Certificate type | First-download SmartScreen behavior (Microsoft's wording) |
| --- | --- |
| Microsoft Store | "✅ No warning — covered by Microsoft's certificate" |
| Valid Certificate (OV/EV) | "⚠️ Warning — app flagged as unrecognized until reputation accumulates; verified publisher name is displayed" |
| No signature | "⚠️ Warning — 'Windows protected your PC'; User must choose 'Run anyway' before the app can run. Enterprise policy can prevent continuation entirely." |
| Self-signed Certificate | "⚠️ Warning — Same behavior as no signature" |

### How long an unknown publisher stays flagged

> "**As downloads accumulate:** SmartScreen reputation builds up automatically. The prompt will stop appearing once the file hash has sufficient download history. There is no exact threshold, but it can take several weeks and hundreds of clean installs from a wide audience."

And there is no way to short-circuit it for consumers:

> "There is no need (or mechanism) to manually submit a file for SmartScreen reputation review for consumer endpoints. Reputation builds organically through download volume."

The only submission channel Microsoft names is for enterprise IT admins via the Microsoft Security Intelligence portal, which "can accelerate trust for internal or managed deployments" — not for the general public. (The Artifact Signing FAQ suggests submitting a signed file to <https://www.microsoft.com/wdsi> "for further review" if prompts persist, which sits slightly awkwardly with the "no mechanism" statement on the SmartScreen page; I read the SmartScreen page as authoritative for consumer endpoints and the FAQ as pointing at the same enterprise/false-positive channel.)

### Two extra behaviours worth planning around

- **Smart App Control** on Windows 11 "may supersede SmartScreen Application Reputation. Smart App Control will block execution of unsigned files unless the file has a positive reputation. Smart App Control signature checks apply to all executable files, not just those downloaded from the Internet." Unsigned is a harder failure on Win11 than the SmartScreen table alone suggests.
- **Auto-updater implication.** Microsoft's advice includes "Do not modify signed files — Avoid modifying files after signing as doing so can break the signature depending on client configuration". An in-app updater that patches or rewrites binaries in place will invalidate the signature it was relying on. Updates should ship as separately-signed artifacts.

Microsoft's own summary of the levers, verbatim: "Publish to the Microsoft Store where feasible"; "Sign every release"; "Use a consistent signing identity — changing your signing certificate affects the publisher trust signal"; and "Communicate with early adopters — for new apps, let beta users know they may see a SmartScreen prompt on first download".

That third one deserves emphasis given §2.1: certificates now expire in ≤460 days, so the signing identity churns. Reputation follows "publisher identity", and CAs typically preserve subject identity across renewal, but Microsoft does not state how identity continuity is computed across a re-key.

## 2.4 Azure Trusted Signing — now **Artifact Signing**

**Naming, first.** Microsoft has renamed the service. The docs now live at `learn.microsoft.com/en-us/azure/artifact-signing/` (canonical URLs on every page; overview `ms.date` 2026-01-02, updated 2026-08-03), the Azure CLI extension is `az extension add --name artifact-signing`, the DevOps task is `AzureArtifactSigning@<version>`, and the GitHub Action is `azure/artifact-signing-action@v2`. The SmartScreen page refers to it as "Artifact Signing (formerly Trusted Signing)". The Azure Resource Manager namespace is still `Microsoft.CodeSigning` and the ARM meters in the billing system are still named "Trusted Signing", so both names are live in different layers. `/azure/trusted-signing/` URLs redirect.

### What it is

> "Artifact Signing is a Microsoft fully managed, end-to-end signing solution that simplifies the certificate signing process and helps partner developers more easily build and distribute applications."
> — <https://learn.microsoft.com/en-us/azure/artifact-signing/overview>

Features Microsoft lists, verbatim: "Provides zero-touch certificate lifecycle management inside FIPS 140-3 level 3 certified HSMs"; "Integrates with leading developer toolsets"; "Supports Public Trust, Private Trust, virtualization-based security (VBS) enclave, code integrity (CI) policy, and test signing scenarios"; "Supports integration with external timestamping services"; "Offers content-confidential signing. Your file never leaves your endpoint, and you get digest signing that is fast and reliable."

For Tiller the relevant profile type is **Public Trust** — chained to a publicly trusted root, which is what a downloaded `.exe` needs.

⚠️ The docs are internally inconsistent about the HSM certification level: the overview and FAQ say **FIPS 140-3 Level 3**, while a note on the certificate management page says "All certificates and keys that you use in Artifact Signing are managed inside FIPS 140-2 Level 3 operated hardware crypto modules." Either satisfies CSBR §6.2.7.4.1, so it doesn't change the answer, but the docs disagree with themselves.

### Cost

The public pricing page **does not render prices** — <https://azure.microsoft.com/en-us/pricing/details/artifact-signing/> shows "$-" for both base price and overage and directs to a quote. Two other Microsoft primary sources do give figures.

The **Azure Retail Prices API** (`https://prices.azure.com/api/retail/prices?$filter=serviceName eq 'Trusted Signing'`), queried 2026-08-23, returns:

| SKU / meter | Retail price | Unit | Effective from |
| --- | --- | --- | --- |
| Basic Account | **9.99 USD** | 1/Month | 2024-06-01 |
| Premium Account | **99.99 USD** | 1/Month | 2024-06-01 |
| Signature Overage | **0.005 USD** | per signature | 2024-06-01 |
| Basic Signature / Premium Signature | 0.0 USD | 1/Month | 2024-06-01 |

The Microsoft Learn SmartScreen page independently corroborates the entry price: "**Cost** — Starts at $9.99/month."

Included quotas from the pricing page: **Basic — 5,000 signatures/month, 1 of each certificate profile type. Premium — 100,000 signatures/month, 10 of each certificate profile type.**

**So: roughly $120/year for Basic, versus $129–$696/year for a conventional OV certificate before token or HSM costs.** Two billing gotchas from the FAQ: "The pricing isn't calculated on a pro rata basis. The invoice is generated with the full amount for the SKU that you selected when you created the account, regardless of when you begin to use the service" — and, more importantly, "**Artifact Signing doesn't support free, trial, or sponsored Azure subscriptions.** To create an Artifact Signing account and certificate profiles, you must have a paid Azure subscription."

### Eligibility — the part that decides whether this is available to a solo maintainer

From the quickstart prerequisites (<https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart>, ms.date 2026-05-21, updated 2026-08-11):

> "Public Trust certificates are available to organizations in the United States, Canada, the European Union, the United Kingdom, Australia, New Zealand, Japan, South Korea, Singapore, Switzerland, Norway, and Israel. **Individual developers must be located in the United States or Canada.** These geographic restrictions do not apply to Private Trust certificates."

**Individual developers are explicitly supported** — there is a dedicated "Identity Validation - Individual Developer" flow. But the geographic restriction on individuals is much narrower than for organizations: **US and Canada only.** An EU-based solo maintainer cannot use the individual path; they would have to qualify as an organization.

The individual flow works like this: identity details are sourced automatically from the Azure billing account ("a billing account with an Account Type of 'Individual' can only be used for individual identity validation"), and identity is proved through Microsoft Entra Verified ID with a third-party verifier (AU10TIX) — email PIN, phone number, a QR-code handoff to a mobile device, government photo ID capture, and a face check. Accepted IDs are "Government-issued IDs such as passports, driving licenses, or ID cards"; "Don't submit privately issued IDs such as library cards, school IDs, club membership cards". Supplementary proof of address may be requested (utility bill or bank statement, "typically within the last three months"). Only the city, state/province and country from the address appear on the certificate; email and street address do not.

The organization flow requires legal business entity name, a website on the entity's domain, a monitored primary email on that domain, a business identifier, business address, and a named individual who then completes the same individual verification. "Processing your identity validation request takes from 1 to 20 business days (possibly longer if we need to request more documentation from you)." Supporting documents "must be issued within the previous 12 months and where the expiration date is a future date that is at least two months away." **There are only three attempts to supply additional documentation**, after which "we can't proceed further with the onboarding" — and a missed email verification link (7-day expiry) means starting a fresh request.

⚠️ **The "3 years of verifiable legal existence" requirement that Trusted Signing carried at launch does not appear anywhere in the current Artifact Signing documentation** I read (overview, quickstart, FAQ, certificate management, signing integrations — all with 2026 dates). I could not confirm whether it was dropped or merely moved somewhere I did not find. This should be verified directly with Microsoft before an organization younger than three years relies on it.

Two further constraints: **"Artifact Signing doesn't issue Extended Validation (EV) certificates. There's no plan to issue EV certificates in the future."** And "you can't use a custom Common Name (CN) or a custom Organization (O)" — the CN is the validated legal entity name, per the CSBRs. If a specific publisher string is required on the certificate, this service cannot provide it.

Identity validation itself expires: "If you don't renew identity validation before the expiration date, certificate renewal stops. All signing processes that are associated with those specific certificate profiles stops… We send multiple reminders starting at 60 days before an identity validation's expiration date."

### How it is driven from CI

Supported integrations, per <https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations>: SignTool, GitHub Actions, Azure DevOps tasks, PowerShell for Authenticode, Azure PowerShell (App Control CI policy), and the Artifact Signing SDK.

**GitHub Actions**, from <https://github.com/Azure/artifact-signing-action>:

```yaml
- name: Azure login
  uses: azure/login@v3
  with:
    client-id: ${{ secrets.AZURE_CLIENT_ID }}
    tenant-id: ${{ secrets.AZURE_TENANT_ID }}
    subscription-id: ${{ secrets.AZURE_SUBSCRIPTION_ID }}

- name: Sign files with Artifact Signing
  uses: azure/artifact-signing-action@v2
  with:
    endpoint: https://eus.codesigning.azure.net/
    signing-account-name: vscx-codesigning
    certificate-profile-name: vscx-certificate-profile
    files-folder: ${{ github.workspace }}\App\App\bin\Release\net8.0-windows
    files-folder-filter: exe,dll
    file-digest: SHA256
    timestamp-rfc3161: http://timestamp.acs.microsoft.com
    timestamp-digest: SHA256
```

The repository states that "it is recommended to use OpenID Connect for authentication". **Under OIDC there is no long-lived secret at all** — `AZURE_CLIENT_ID`, `AZURE_TENANT_ID` and `AZURE_SUBSCRIPTION_ID` are identifiers, not credentials. GitHub's own documentation confirms the model: "OpenID Connect (OIDC) allows your GitHub Actions workflows to access resources in Azure, without needing to store the Azure credentials as long-lived GitHub secrets", requiring `permissions: id-token: write` and warning that "you **must** define at least one condition, so that untrusted repositories can't request access tokens for your cloud resources" (<https://docs.github.com/en/actions/how-tos/secure-your-work/security-harden-deployments/oidc-in-azure>).

**SignTool** on a self-hosted or hosted Windows runner, from the same Learn page:

```console
& "<Path to SDK bin folder>\x64\signtool.exe" sign /v /debug /fd SHA256 /tr "http://timestamp.acs.microsoft.com" /td SHA256 /dlib "<Path to Artifact Signing dlib bin folder>\x64\Azure.CodeSigning.Dlib.dll" /dmdf "<Path to metadata file>\metadata.json" <File to sign>
```

with `metadata.json`:

```json
{
  "Endpoint": "<Artifact Signing account endpoint>",
  "CodeSigningAccountName": "<Artifact Signing account name>",
  "CertificateProfileName": "<Certificate profile name>",
  "CorrelationId": "<Optional CorrelationId value>"
}
```

Prerequisites: SignTool from Windows SDK **10.0.2261.755 or later** ("20348 Windows SDK version isn't supported with our dlib"), .NET 8 Runtime, VC++ Redistributable, and the dlib — installable in one step via `winget install -e --id Microsoft.Azure.ArtifactSigningClientTools`. Authentication is `DefaultAzureCredential`, so on a hosted runner it picks up the OIDC/az-login token, and on an Azure VM it can use a user-assigned managed identity — no secret in either case. Outside Azure (the FAQ names Google Cloud Platform explicitly), the fallback is `EnvironmentCredential` with `AZURE_TENANT_ID`, `AZURE_CLIENT_ID` and `AZURE_CLIENT_SECRET` — **that last one is a real long-lived secret** and is the shape to avoid if OIDC is available.

The `Endpoint` must match the account's Azure region; a mismatch "commonly causes a 403 Forbidden error". Available regions include East US, West US/2/3, West Central US, Central US, North Central US, South Central US, North Europe, West Europe, Poland Central, Switzerland North, Brazil South, Japan East and Korea Central.

The RBAC role needed to sign is **Artifact Signing Certificate Profile Signer**; the role needed to create the identity validation is **Artifact Signing Identity Verifier**.

---

# Part 3 — Both platforms

## 3.1 What must be held as a CI secret, and the blast radius if it leaks

### macOS

| Secret | What it is | Blast radius if leaked |
| --- | --- | --- |
| **Developer ID Application `.p12`** (certificate + private key) + its export passphrase | The actual signing key. Apple's model is a file on disk, imported into a temporary keychain on the runner. | **Worst case on either platform.** An attacker can sign arbitrary Mac software as us. Malware signed with our identity would pass the Developer ID check. Remediation is revocation — and per Apple, "Any Developer ID app signed with a certificate that has been revoked can no longer be installed nor launch if it's already installed." **Revoking kills every already-shipped Tiller build in the field, not just the malicious one.** |
| **App Store Connect API key `.p8`** + Key ID + Issuer ID (must be a **Team** key) | Notarization credential. Downloadable once only; Apple keeps no copy. | An attacker can submit software for notarization *under our team*. They cannot sign — notarization requires an already-Developer-ID-signed binary — so on its own this is a reputational/abuse exposure rather than a signing compromise. Apple: "If you suspect a private key is compromised, immediately revoke the key in App Store Connect." Revocation here is cheap; it does not touch shipped builds. |
| *or* **Apple ID + app-specific password + Team ID** | Alternative notarization credential. | Scoped to the Apple Account. Cap of 25 active; individually revocable. Note the operational hazard: changing the primary Apple Account password auto-revokes all of them and breaks the pipeline. |

**The asymmetry is the point.** On macOS the signing key is unavoidably a file that CI must possess, and its compromise is catastrophic *and* its remediation is catastrophic. Mitigations worth considering (none verified against Apple docs as officially recommended): keep signing off hosted runners; scope the secret to a protected environment with required reviewers; treat the `.p12` passphrase as a separate secret; use a Team API key for notarization so the notarization credential is independently revocable.

### Windows

The shape depends entirely on which of the three §2.2 options is chosen.

| Option | CI secret | Blast radius |
| --- | --- | --- |
| **USB token** | None (the key is physically absent from CI) | No CI secret to leak. The exposure is physical theft plus the token PIN. Incompatible with hosted runners. |
| **Own cloud HSM** | Cloud credentials for the HSM | An attacker with them can invoke signing. The key itself is non-exfiltrable — they can sign, but they cannot walk away with a reusable key. |
| **Signing service (Artifact Signing, eSigner, KeyLocker)** | An API credential — or, with GitHub OIDC to Azure, **nothing at all** | An attacker who compromises the workflow can sign while the compromise lasts. The key is never exportable ("Artifact Signing does *not* support importing or exporting private keys and certificates"). |

**The signing-service model, driven by OIDC, is the only configuration in this document where there is no long-lived signing secret in CI.** That is a structural difference, not a marginal one.

Artifact Signing's short-lived certificates further contain the damage. Because certificates last 72 hours and are reissued daily, revocation can be surgical:

> "if it's determined that a subscriber signed code that was malware or a potentially unwanted application (PUA) …, revocation actions can be isolated to revoking only the certificate that signed the malware or PUA. The revocation affects only the code that was signed by using that certificate on the day that it was issued. The revocation doesn't apply to any code that was signed before that day or after that day."
> — <https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management>

Contrast the macOS Developer ID case, where revocation is all-or-nothing across everything ever signed with that certificate. **This is the sharpest single contrast in the whole document.**

Microsoft also monitors and will act unilaterally: "For a confirmed case of misuse or abuse, Artifact Signing immediately takes the necessary steps to mitigate and remediate any threats, including targeted or broad certificate revocation and account suspension."

### Applies to both

- Anything a workflow can read, a malicious PR or a compromised dependency in the build can read. Signing steps should live in a job that does not run untrusted code, gated behind an environment with manual approval.
- Both platforms punish identity churn. Apple caps Developer ID Application certificates at five per team; Microsoft warns "changing your signing certificate affects the publisher trust signal". Rotating a leaked identity is not free even after the incident is over.

## 3.2 Renewal, expiry, revocation — and why timestamping decides the outcome

### Windows

**Timestamp, always.** Microsoft states it as an imperative:

> "**Always time-stamp your Authenticode signatures. Without a time stamp, the signature becomes invalid when the signing certificate expires, and Windows will treat the binary as unsigned. Time stamping ensures long-term signature validity.**"
> — <https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures>

The mechanism, from the same page: "The countersignature method of time stamping implemented below allows for signatures to be verified even after the signing certificate has expired or been revoked. The time stamp allows the verifier to reliably know the time that the signature was affixed and thereby trust the signature if it was valid at that time."

Microsoft's recommended parameters: SHA-256 digest (`/fd SHA256`), RFC 3161 timestamping (`/tr` with `/td SHA256`) rather than the legacy `/t` protocol. "Do not use SHA-1 as the sole signing algorithm for new releases."

**Expiry.** A timestamped, signed binary keeps validating indefinitely after the certificate expires. Given §2.1's 460-day cap, this is what makes annual certificate churn survivable: **already-released builds do not break when the certificate expires.** New builds simply need the new certificate.

**Revocation.** This is where the two Microsoft sources appear to disagree and the CA/Browser Forum resolves it.

- The Win32 timestamping page says a timestamp allows verification "even after the signing certificate has expired **or been revoked**".
- Artifact Signing's certificate management page says a timestamped signature "is valid long after the signing certificate and the TSA certificate expire (**unless either are revoked**)."

The reconciling rule is in CSBR v3.11.0 §4.9.6 (spaces restored):

> "A Certificate MAY have a one-to-one relationship or one-to-many relationship with the signed Code. Regardless, revocation of a Certificate may invalidate the Code Signatures on all signed Code, some of which could be perfectly sound. Because of this, the CA MAY specify the time at which the Certificate is first considered to be invalid in the `revocationDate` field of a CRL entry or the `revocationTime` field of an OCSP response to time-bind the set of software affected by the revocation, and software should continue to treat objects containing a timestamp dated before the revocation date as valid."

And §4.9.5 adds that for key-compromise events "this date SHOULD be the earliest date of suspected compromise."

**So the honest answer is: it depends on the revocation date the CA sets.** A revocation dated *after* our release leaves timestamped builds signed before that date valid. A revocation dated to the earliest suspected compromise — which is what a key-compromise incident calls for — invalidates everything timestamped after that point. Note the CSBR says "MAY" and "should", not "MUST": this is a discretionary CA behaviour and a client-side convention, not a guarantee. **Timestamping is a strong defence against expiry and a partial, CA-dependent defence against revocation.**

### macOS

Apple requires a secure timestamp as a precondition of notarization at all (requirement 4 in §1.3), so a notarized build is always timestamped.

**Expiry.** Apple's Account Help is explicit:

> "Gatekeeper will evaluate the validity of your Developer ID certificate when your application is installed. As long as your Developer ID certificate was valid when you compiled your app, then users can download and run your app, even after the expiration date of the certificate. However, you'll need a new certificate to sign updates and new applications."
> — <https://developer.apple.com/help/account/create-certificates/create-developer-id-certificates>

The certificates reference page says the same: "If your certificate expires, users can still download, install, and run versions of your Mac applications that were signed with this certificate."

**Revocation.** No such grace:

> "Any Developer ID app signed with a certificate that has been revoked can no longer be installed nor launch if it's already installed."
> — same page

This is more severe than the Windows equivalent: no revocation-date time-binding is documented, and it reaches *already-installed* copies, not just new downloads. Apple's end-user article confirms the user-visible consequence: "If macOS detects that software has malicious content or its authorization has been revoked for any reason, your Mac notifies you that the app will damage your computer."

**One asymmetry to watch if Tiller ever uses a Developer ID provisioning profile** (needed only for advanced capabilities such as CloudKit or Push): Gatekeeper then "will evaluate the validity of your Developer ID provisioning profile at every app launch" and "if your Developer ID provisioning profile expires, the app will no longer launch." Profiles generated after 2017-02-22 are valid for 18 years, so this is a long fuse rather than an annual one — but it is a per-launch check, unlike the certificate's install-time check. Tiller has no current need for such capabilities; if that changes, this becomes a live constraint.

### Renewal cadence, side by side

| | macOS | Windows |
| --- | --- | --- |
| Recurring cost | Apple Developer Program **99 USD/year** | Certificate or service subscription — see §2.1 / §2.4 |
| Certificate term | Not published by Apple (see gaps) | **≤460 days** for certs issued on/after 2026-03-01 (CSBR §6.3.2); Artifact Signing: 72 hours, auto-renewed daily |
| Lapse consequence | Program lapse ⇒ certificates cease to function for new signing; existing signed builds unaffected | Certificate expiry ⇒ cannot sign new builds; timestamped existing builds unaffected |
| Expiry hits shipped builds? | **No** (if signed while valid) | **No** (if timestamped) |
| Revocation hits shipped builds? | **Yes — including already-installed copies** | **Depends on the revocation date** the CA sets (CSBR §4.9.6); Artifact Signing scopes it to a single day's signings |

---

# Part 4 — What I could not establish

Stated plainly, because an honest gap is more useful than a confident guess.

1. **The validity period of a Developer ID Application certificate.** Apple documents the 18-year validity of Developer ID *provisioning profiles* and the WWDR/Developer ID *intermediate* certificate expiry dates (<https://developer.apple.com/support/expiration/>), but I found no Apple page stating how long a Developer ID Application leaf certificate is valid. It is visible on an issued certificate; it is not, as far as I could find, published. Everything in §3.2 about macOS expiry holds regardless of the number.

2. **The exact dialog string for the signed+notarized first launch (Case A).** Apple describes it ("your Mac asks if you're sure that you want to open it") and states that Gatekeeper "places descriptive information in the initial launch dialog", but publishes no screenshot or literal string for that case in the current article. The Case B string is recoverable from Apple's own image alt text and is quoted verbatim in §1.4; Case A is described in Apple's words but not quoted, because Apple does not quote it.

3. **Whether Artifact Signing still requires three years of verifiable legal existence for organizations.** That requirement was widely documented for Trusted Signing at launch. It does not appear in the current Artifact Signing overview, quickstart, FAQ, or certificate-management pages (all dated 2026). I could not determine whether it was removed or relocated. **Verify with Microsoft before a young entity relies on this.**

4. **DigiCert's monthly-vs-annual price discrepancy.** $44/mo does not multiply to the $696 subscription shown on the same page, nor $62/mo to $972. The page offers no explanation. Both figures are recorded as displayed.

5. **SSL.com's YubiKey price and Google Cloud HSM attestation fee.** Two SSL.com pages disagree ($379 vs $249 for the token; $500 vs $1,500 for Google Cloud HSM attestation). I could not determine which is current.

6. **Artifact Signing's actual price on its own pricing page.** The page renders "$-" placeholders. The $9.99 / $99.99 / $0.005 figures come from the Azure Retail Prices API and are corroborated for the entry tier by Microsoft Learn, but they carry an `effectiveStartDate` of 2024-06-01 and I could not confirm from the pricing page itself that they are still current as displayed to a purchaser.

7. **Certum's eligibility rules for the €49 Open Source Code Signing product** — what evidence of open-source status is required, and whether it issues to an individual or requires a registered entity. The shop page describes the audience but not the criteria.

8. **GlobalSign's prices.** Not published; the page shows "From / year" with no figure. Not filled in from a reseller, by the rules of this exercise.

9. **How SmartScreen publisher reputation carries across a certificate renewal or re-key.** Microsoft says reputation attaches to "publisher identity" and warns that "changing your signing certificate affects the publisher trust signal", but does not define whether a renewed certificate with the same validated subject counts as the same identity. Given the new ≤460-day certificate lifetime this is now a recurring question for every Windows publisher, and Microsoft does not answer it.

---

## Source index

**Apple**
- Apple Developer Program enrolment and fees — <https://developer.apple.com/support/enrollment/>
- Apple Developer Program — <https://developer.apple.com/programs/>
- Certificate types reference — <https://developer.apple.com/support/certificates/>
- Create Developer ID certificates (Account Help) — <https://developer.apple.com/help/account/create-certificates/create-developer-id-certificates>
- Developer ID — <https://developer.apple.com/developer-id/>
- Notarizing macOS software before distribution — <https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution>
- Customizing the notarization workflow — <https://developer.apple.com/documentation/security/customizing-the-notarization-workflow>
- TN3147: Migrating to the latest notarization tool — <https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool>
- Submitting software for notarization over the web — <https://developer.apple.com/documentation/notaryapi/submitting-software-for-notarization-over-the-web>
- Creating API keys for App Store Connect API — <https://developer.apple.com/documentation/appstoreconnectapi/creating-api-keys-for-app-store-connect-api>
- Certificate expiration — <https://developer.apple.com/support/expiration/>
- "Upcoming changes to macOS Sequoia" developer news, 2024-08-06 — <https://developer.apple.com/news/?id=saqachfa>
- Safely open apps on your Mac, published 2026-05-27 — <https://support.apple.com/en-us/102445>
- Open a Mac app from an unknown developer (Mac User Guide) — <https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac>
- App-specific passwords, published 2025-10-08 — <https://support.apple.com/en-us/102654>
- Gatekeeper and runtime protection (Apple Platform Security) — <https://support.apple.com/guide/security/gatekeeper-and-runtime-protection-sec5599b66df/web>

**CA/Browser Forum**
- Code Signing requirements index — <https://cabforum.org/working-groups/code-signing/documents/>
- Code Signing Baseline Requirements v3.11.0 (PDF) — <https://cabforum.org/uploads/CA-Browser-Forum-CSCBR-3.11.0.pdf>

**Microsoft**
- What is Artifact Signing? — <https://learn.microsoft.com/en-us/azure/artifact-signing/overview>
- Quickstart: Set up Artifact Signing — <https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart>
- Artifact Signing certificate management — <https://learn.microsoft.com/en-us/azure/artifact-signing/concept-certificate-management>
- Set up signing integrations — <https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations>
- Artifact Signing FAQ — <https://learn.microsoft.com/en-us/azure/artifact-signing/faq>
- Artifact Signing pricing — <https://azure.microsoft.com/en-us/pricing/details/artifact-signing/>
- Azure Retail Prices API — `https://prices.azure.com/api/retail/prices?$filter=serviceName eq 'Trusted Signing'`
- SmartScreen reputation for Windows app developers — <https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation>
- Time Stamping Authenticode Signatures — <https://learn.microsoft.com/en-us/windows/win32/seccrypto/time-stamping-authenticode-signatures>
- azure/artifact-signing-action — <https://github.com/Azure/artifact-signing-action>

**GitHub**
- Configuring OpenID Connect in Azure — <https://docs.github.com/en/actions/how-tos/secure-your-work/security-harden-deployments/oidc-in-azure>

**Certificate authorities (own pricing pages)**
- DigiCert code signing — <https://www.digicert.com/signing/code-signing-certificates>
- SSL.com IV / OV / EV code signing — <https://www.ssl.com/products/software-integrity/code-signing/iv/>, `/ov/`, `/ev/`
- SSL.com eSigner — <https://www.ssl.com/esigner/>
- Certum shop, code signing — <https://shop.certum.eu/code-signing.html>
- GlobalSign code signing — <https://www.globalsign.com/en/code-signing-certificate>

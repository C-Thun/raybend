RayBend user guide

Get started
Create a photo library in Import, or register a directory containing an existing .raybend library. Invalid paths, permissions and damaged libraries are reported during probing or submission. Do not delete library files to bypass an error.

Import and browse
Choose sources and destination, check exclusions and naming, then import. File writes follow your chosen operation. Review the batch error list if anything fails. Browse, rate, tag, filter and view photos. The selector above shooting information changes the displayed issue for this browse session without writing latest. Leaving Browse or changing the directory restores latest. Ctrl+K finds actions and their current shortcuts; shortcut settings let you change bindings.

Non-destructive editing
Edits and issues live in the library without overwriting originals. Wait for saving before quitting or updating. Complete color management is planned for a later stage.

Export
The gallery shows latest, named issues and SOOC, merging identical content. Select the issues, add or update and save a preset, then enqueue and start it. Unsaved settings cannot start. Ordinary export supports WebP, AVIF, JPG and PNG; PNG has no quality control. Choose original size, a percentage or a maximum edge, plus the destination and naming template. By default Enter enqueues the selection and Ctrl+Enter starts/stops the selected preset. Ctrl+K shows the current, customizable shortcuts.

Up to four presets can stay enabled, each with its own background queue. Empty enabled queues wait for new items. Stop preserves the queue and lets an in-flight photo finish. Failed items show their reason and offer retry. Switching workspaces does not stop output. Queues and switches are held in memory and lost on exit; preset settings persist on this device. Confirmed reset clears queues, switches and processing records without forcibly interrupting an in-flight render. Invalidated issues are skipped before execution; changed source files cause an error. Original photos are not overwritten, and conflicting output names receive a sequence suffix.

External editor handoff
Use the entry on the right of the Browse toolbar. It captures the currently displayed issue. Choose a registered application and output directory, then confirm. Add Application discovers supported installed candidates; portable applications can be registered by choosing an exe manually. Applications and the last directory persist. Uninstalled or unavailable applications are reported and removed from the registration list.

RayBend writes an original-size RGB16 TIFF using the shared export rendering, geometry and metadata pipeline, then launches the chosen application. Closing the dialog or switching workspaces lets preparation continue. Cancellation takes effect at stage boundaries, and an already written TIFF remains. If launching fails, the saved path remains available for manual opening. This is a one-way handoff: no automatic return, no registration of the TIFF in the library, and no changes to the source or latest. Supported metadata fields are retained; full MakerNote, embedded thumbnails and ICC preservation are not promised. Color management remains planned for a later stage.

Installation and updates
Use the official HTTPS website and its GitHub Release links. Check the version, channel and SHA-256. Updates connect only when you request a check. Downloads require a valid update signature; finish imports (including paused batches), disable export presets, finish external handoff jobs, and wait for edits and database migrations before installation. Retry failed downloads. Recover installer failures with the same or a newer full installer.

Windows trust
Publisher signatures, SmartScreen reputation and website authenticity provide different assurances. An unsigned or unfamiliar app may be warned about or blocked. Smart App Control and organization policies may not offer an override. Disabling protection is not a standard installation step. A traditional installer is not Store-certified unless explicitly stated.

Data and recovery
Photos and catalog live in the library you chose. Device settings, cache and app.db use the local application data directory. The installer does not add a hook to delete photo libraries. Real uninstall behavior must be verified for each installer type. Installing an older executable does not revert a migrated database: older apps reject newer schemas. Keep migration snapshots and independent backups.

Support and privacy
Open an issue using the repository link in About. Include the version, Windows version, steps and error code. The copied diagnostic summary contains build and runtime information without photo paths, filenames, usernames or library records. Check screenshots, logs, EXIF and exported error lists before sharing. There is no automatic telemetry or diagnostic upload.

Support the project
Report issues, contribute improvements or help others. No donation account has been configured; the app does not invent payment links.

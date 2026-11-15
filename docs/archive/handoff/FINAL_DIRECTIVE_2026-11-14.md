# EAK FINAL PRODUCT DELIVERY DIRECTIVE — TOMORROW 7:00 AM

Issued by the project owner on 2026-11-13 (~20:35 IST). Target: 2026-11-14 07:00 IST. Recorded verbatim.

Firstmate and Claude Code, this is the final delivery instruction.

**My target is a functional, installable EAK desktop application by 7:00 AM tomorrow.** Treat this as the highest-priority delivery deadline.

I want an actual electronics engineering application, not a collection of UI mockups, demonstrations or placeholder screens.

## 1. PRIORITY ZERO — UI/UX FIRST (NON-NEGOTIABLE)

Before implementing additional backend features, establish the complete visual and interaction architecture.

I want EAK to have a premium, professional, Cursor-inspired interface specifically designed for electronics engineers.

The UI/UX must include:
- Professional dark and light themes.
- Excellent CMF: colour, material and finish.
- Clean typography and visual hierarchy.
- Project navigation and workspace management.
- Contextual toolbars and property panels.
- Command palette and keyboard shortcuts.
- Component search and library navigation.
- Integrated AI assistant.
- Engineering warnings and verification panels.
- Resizable panels and efficient canvas space.
- Beginner-friendly workflows and advanced engineering controls.

The schematic and PCB editors must be the centre of the application.

Do not build a generic AI dashboard. Do not make engineers navigate complicated software-development concepts to design electronics.

`eak-ux` owns design direction and design-system acceptance. `eak-desktop` implements it. Coordinate before modifying shared files.

## 2. PRIORITY ONE — NATIVE SCHEMATIC EDITOR

Build a genuinely functional schematic editor connected to the Rust engineering core.

Mandatory capabilities:
- Interactive component placement, movement and deletion.
- Symbol and component library integration.
- Wire drawing and editing.
- Net and pin connectivity.
- Power symbols and net labels.
- Component properties and values.
- Multiple schematic sheets where supported.
- Zoom, pan, selection and grid controls.
- Undo and redo.
- Save, reopen and revision management.
- Live connectivity feedback.
- ERC and actionable error reporting.
- AI-assisted circuit creation through approved commands.

A visual wire crossing must not automatically imply electrical connectivity. Junctions, pins, net identities and connectivity must be represented correctly in EAK-IR.

**The editor must create, save, reopen and verify actual electrical designs.**

## 3. PRIORITY TWO — NATIVE PCB EDITOR

Build a genuinely functional PCB editor, not a static canvas.

Mandatory capabilities:
- Board outline creation and editing.
- Component placement, rotation and movement.
- Footprint and pad representation.
- Layer management.
- Track routing and editing.
- Vias.
- Net highlighting.
- Unrouted-net visibility.
- Copper zones and ground planes.
- Design-rule configuration.
- Clearance and trace-width validation.
- Connectivity checking.
- Undo and redo.
- Save, reopen and revision management.
- DRC feedback with actionable corrections.
- Manufacturing export preparation.

The PCB editor must be connected to the actual EAK project model and engineering commands.

Prioritise dependable two-layer PCB functionality before adding advanced multilayer features. Do not fake completed routing or copper operations.

## 4. ENGINEERING BACKEND — RUST FIRST

The interface must be built against the approved engineering architecture.

- Rust engineering core.
- Canonical versioned EAK-IR.
- Shared typed Rust command system.
- Tauri 2 desktop application.
- React + TypeScript frontend.
- Vite.
- SQLite for metadata, indexing and cache.

Human and AI actions must use the same command interfaces.

Preserve project integrity, revisions, transactions, undo/redo, validation and recovery.

No Python anywhere in the EAK product or toolchain. Do not replace Tauri with Electron or change the approved stack.

UI first means establishing the real design system and working interaction architecture first—not creating disconnected mockups and postponing integration.

## 5. Distributor ELECTRONICS — MANDATORY

Study the existing DESK repository:

https://github.com/NijoP/DESK

Especially:
- `desk/component_intelligence/`
- `library/`
- `docs/ecosystem/DISTRIBUTOR_INTEGRATION_ARCHITECTURE.md`
- `docs/ecosystem/FABRICATOR_MANUFACTURING_ARCHITECTURE.md`

Implement Distributor component sourcing and library integration.

Required functionality:
- Component search.
- Distributor part-number identification.
- Manufacturer part-number mapping.
- Datasheet retrieval.
- Electrical specification inspection.
- Footprint and package matching.
- Component availability and pricing where accessible.
- BOM generation.
- Alternative component discovery.
- Component provenance and verification status.

Never invent component specifications or claim that a component is verified without evidence.

Keep Distributor sourcing separate from the native EDA engine.

Fabricator manufacturing preparation should remain available through the previously defined integration architecture. Do not confuse Distributor component sourcing with Fabricator fabrication and assembly.

## 6. AI MODEL AND AGENT INTEGRATION

Preserve the universal model-adapter architecture.

Users must be able to connect supported providers such as Claude, OpenAI, Grok, Kimi, Gemini and other compatible models through secure authorization.

AI must be able to:
- Understand project requirements.
- Research components.
- Propose schematics.
- Create and modify designs through typed commands.
- Assist with PCB placement and routing.
- Run verification.
- Explain errors and propose corrections.

The engineer must retain appropriate approval controls. No model may bypass engineering validation.

## 7. CROSS-PLATFORM INSTALLATION — MANDATORY

Deliver native installation packages for:

- Windows 10/11.
- Linux, including Ubuntu.
- macOS.

Use the same approved Rust/Tauri architecture across platforms.

Requirements:
- Build and package each platform independently.
- Validate application startup and basic project workflows.
- Verify native dependencies and runtime requirements.
- Provide exact installation instructions.
- Provide one straightforward installation command per supported platform where technically appropriate.
- Include version information and release notes.

Do not report a platform as supported merely because its source code compiles. Verify its actual package and launch process.

If a platform build requires another machine or CI runner, use the available CI infrastructure rather than pretending it was tested locally.

## 8. TWO END-TO-END ACCEPTANCE PROJECTS

Use the application itself to validate:

**Project A:** Complete ESP32 dashboard hardware and firmware project.

**Project B:** Power electronics PCB project.

Both must use the native schematic and PCB workflows, component data, EAK-IR and engineering verification.

Generate applicable BOMs, schematics, PCB files, Gerbers and verification reports.

These projects validate the general-purpose platform. Do not hardcode EAK around ESP32 or buck converters.

Keep RF and other domain capabilities in the general-purpose architecture, even if their advanced validation is deferred.

## 9. EXPLICITLY DEFER 3D

For this release, defer the advanced 3D workspace and 3D rendering implementation.

Do not spend the delivery window building a 3D engine.

Preserve the architecture so it can be added later without redesigning the schematic and PCB editors.

Do not defer schematic editing, PCB editing, UI/UX, Distributor integration or cross-platform packaging.

## 10. EXECUTION AND RESOURCE MANAGEMENT

- Use the existing Secondmates according to their specialisations.
- Use additional temporary Crewmates only where genuinely useful.
- Keep one owner per shared implementation area.
- Integrate continuously rather than waiting until the end.
- Run targeted tests after every significant change.
- Fix failed CI and merge blockers immediately.
- Preserve the approved technology stack.
- Use the existing GitHub repository and its branch protections.
- Do not force-push or overwrite unrelated work.
- Avoid unnecessary repeated planning and idle token consumption.
- Monitor RAM and process usage on the development laptop.

Do not let the localhost dashboard, terminal tooling or agent-management features consume the time needed to finish the actual EDA application.

## 11. FINAL RELEASE ACCEPTANCE

At 7:00 AM, I expect:

1. An installable EAK desktop application.
2. A professional, consistent UI/UX.
3. A functional native schematic editor.
4. A functional native PCB editor.
5. Real EAK-IR integration and Rust engineering commands.
6. Working ERC and DRC capabilities for the supported scope.
7. Distributor component sourcing and library integration.
8. Applicable Fabricator manufacturing preparation.
9. Both acceptance projects with engineering evidence.
10. Windows, Linux and macOS packages with verified status.
11. A complete requirements and maturity matrix.
12. Installation and launch instructions.

Report separately:
- Implemented features.
- Tested features.
- Independently verified features.
- Unsupported features.
- Known defects.
- Platform-specific limitations.

**Do not declare production readiness based on screenshots, passing frontend tests or a successful build alone.**

The deadline is fixed. Prioritise the actual product and its two primary editors. If a capability is incomplete, disclose it honestly rather than implementing a fake substitute.

**FINAL PRODUCT VISION: EAK must feel like a premium, professional electronics engineering environment, with the usability of Cursor and the engineering depth of a serious EDA platform. Build the real application.**

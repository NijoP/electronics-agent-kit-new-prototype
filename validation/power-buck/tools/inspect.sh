#!/bin/sh
# Independent reviewer tool (NOT EAK functionality): inspect the EAK manufacturing package
# with plain POSIX tools, independently of EAK's own self-validation.
# Usage: tools/inspect.sh <outputs-dir> <board-width-mm> <board-height-mm>
# Prints one "PASS|FAIL|INFO <check>: <detail>" line per check; exits 1 if any FAIL.
set -eu
out="$1"; bw="$2"; bh="$3"
fail=0
say() { echo "$1 $2"; [ "$1" = "FAIL" ] && fail=1; return 0; }

for f in "$out"/*.g*; do
    n=$(basename "$f")
    # Structure: format, units, end-of-file.
    if grep -q '^%FSLAX46Y46\*%' "$f" && grep -q '^%MOMM\*%' "$f" && tail -n 1 "$f" | grep -q '^M02\*'; then
        say PASS "gerber-structure $n: FSLAX46Y46, MOMM, terminated by M02"
    else
        say FAIL "gerber-structure $n: missing format/units/M02"
    fi
    # Every D-code used must be defined.
    undefined=$(awk '/^%ADD/ { s=$0; sub(/^%ADD/, "", s); sub(/[A-Z].*/, "", s); def["D" s]=1 }
                     /^D[1-9][0-9]+\*$/ { d=$0; sub(/\*$/, "", d); if (!(d in def)) print d }' "$f" | sort -u | tr '\n' ' ')
    if [ -z "$undefined" ]; then say PASS "gerber-apertures $n: every selected aperture is defined"; else say FAIL "gerber-apertures $n: undefined $undefined"; fi
    # Coordinate extents (format 4.6 -> mm) must lie on the board.
    ext=$(awk '/^X-?[0-9]+Y-?[0-9]+D0[123]\*$/ { s=$0; sub(/^X/, "", s); split(s, a, "Y"); y=a[2]; sub(/D.*/, "", y);
                 x=a[1]/1e6; y=y/1e6; if (n==0||x<x0)x0=x; if(n==0||x>x1)x1=x; if(n==0||y<y0)y0=y; if(n==0||y>y1)y1=y; n++ }
               END { if (n==0) print "none"; else printf "%.3f %.3f %.3f %.3f %d\n", x0, x1, y0, y1, n }' "$f")
    if [ "$ext" = "none" ]; then
        say INFO "gerber-extent $n: no coordinates (empty layer)"
    else
        echo "$ext" | awk -v bw="$bw" -v bh="$bh" -v n="$n" '{ ok = ($1 >= 0 && $2 <= bw && $3 >= 0 && $4 <= bh);
            printf "%s gerber-extent %s: x %.3f..%.3f y %.3f..%.3f mm over %d coordinates (board %sx%s)\n", ok ? "PASS" : "FAIL", n, $1, $2, $3, $4, $5, bw, bh }'
        echo "$ext" | awk -v bw="$bw" -v bh="$bh" '{ exit !($1 >= 0 && $2 <= bw && $3 >= 0 && $4 <= bh) }' || fail=1
    fi
done

drl=$(ls "$out"/*.drl | head -n 1)
awk '/^T[0-9]+C/ { t=$0; sub(/C.*/, "", t); d=$0; sub(/^T[0-9]+C/, "", d); dia[t]=d }
     /^T[0-9]+$/ { cur=$0 }
     /^X/ { cnt[cur]++ }
     END { for (t in cnt) printf "INFO drill-tools %s: %s mm x %d holes\n", t, dia[t], cnt[t] }' "$drl" | sort
holes=$(grep -c '^X' "$drl")
say INFO "drill-total: $holes plated holes"

bom=$(ls "$out"/*BOM.csv | head -n 1)
cpl=$(ls "$out"/*CPL.csv | head -n 1)
bom_qty=$(awk -F'","|,' 'NR>1 { q += $NF } END { print q }' "$bom")
cpl_rows=$(awk 'NR>1' "$cpl" | grep -c .)
if [ "$bom_qty" = "$cpl_rows" ]; then say PASS "bom-vs-cpl: BOM quantity $bom_qty = CPL rows $cpl_rows"; else say FAIL "bom-vs-cpl: BOM quantity $bom_qty != CPL rows $cpl_rows"; fi
bad_cpl=$(awk -F, -v bw="$bw" -v bh="$bh" 'NR>1 { x=$2; y=$3; sub(/mm/, "", x); sub(/mm/, "", y); x+=0; y+=0; if (x<0||x>bw+0||y<0||y>bh+0) print $1 }' "$cpl" | tr '\n' ' ')
if [ -z "$bad_cpl" ]; then say PASS "cpl-on-board: every placement centre lies on the board"; else say FAIL "cpl-on-board: off-board $bad_cpl"; fi
no_mpn=$(awk 'NR>1 && /,"",[0-9]+$/ { split($0, q, "\""); print q[4] }' "$bom" | tr '\n' ' ')
say INFO "bom-mpn: lines without manufacturer part number: ${no_mpn:-none}"

# Manifest hashes, recomputed with the system sha256sum (independent of EAK's sha2 crate).
if command -v sha256sum >/dev/null 2>&1; then
    mism=0
    for f in "$out"/*; do
        n=$(basename "$f"); [ "$n" = "manifest.json" ] && continue
        h=$(sha256sum "$f" | cut -d' ' -f1)
        grep -q "$h" "$out/manifest.json" || { say FAIL "manifest-hash $n: $h not in manifest"; mism=1; }
    done
    [ $mism -eq 0 ] && say PASS "manifest-hash: every file's sha256sum matches manifest.json"
else
    say INFO "manifest-hash: sha256sum not available; skipped"
fi
exit $fail

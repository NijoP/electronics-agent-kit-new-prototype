#!/bin/sh
# Independent reviewer tool (NOT EAK functionality): render the EAK manufacturing package
# to SVG views for visual inspection, and print simple file statistics.
# Usage: tools/render.sh <outputs-dir> <svg-out-dir>
set -eu
out="$1"
dst="$2"
here=$(dirname "$0")
mkdir -p "$dst"

layer() { # file color opacity
    awk -v color="$2" -v opacity="$3" -f "$here/gerber2svg.awk" "$1"
}
frag() { ls "$out"/*"$1" 2>/dev/null | head -n 1; }

view() { # name, then "suffix:color:opacity" items
    name="$1"; shift
    {
        echo "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"${VB:--2 -35 70 37}\" width=\"1400\" height=\"${VH:-740}\">"
        echo '<rect x="-100" y="-100" width="300" height="300" fill="#102010"/>'
        for spec in "$@"; do
            suffix=${spec%%:*}; rest=${spec#*:}; color=${rest%%:*}; opacity=${rest#*:}
            f=$(frag "$suffix")
            if [ "$suffix" = "PTH.drl" ]; then
                awk -v drill=1 -f "$here/gerber2svg.awk" "$f"
            else
                layer "$f" "$color" "$opacity"
            fi
        done
        echo '</svg>'
    } > "$dst/$name.svg"
    echo "$dst/$name.svg: $(grep -o 'stats[^-]*' "$dst/$name.svg" | tr '\n' ';')"
}

view top "B_Cu.gbl:#3060c0:0.35" "F_Cu.gtl:#d08030:0.85" "F_Silkscreen.gto:#ffffff:0.9" "Edge_Cuts.gm1:#ffff00:1" "PTH.drl::"
view bottom "B_Cu.gbl:#3060c0:0.9" "Edge_Cuts.gm1:#ffff00:1" "PTH.drl::"
view top-copper "F_Cu.gtl:#d08030:1" "Edge_Cuts.gm1:#ffff00:1" "PTH.drl::"
view top-mask-paste "F_Mask.gts:#40ff40:0.6" "F_Paste.gtp:#c0c0c0:0.9" "Edge_Cuts.gm1:#ffff00:1"

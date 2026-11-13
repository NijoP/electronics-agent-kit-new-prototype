# Independent reviewer tool: render the RS-274X / Excellon subset that EAK emits to SVG.
#
# This is NOT EAK functionality and is not used by EAK. It exists so a reviewer can look
# at the actual fabrication files, independently of EAK's internal geometry. It parses only
# what it sees: %FS...X46Y46% coordinates in mm, %ADD..C/R..% apertures, D01/D02/D03,
# G36/G37 regions, and Excellon T<n>C<d> tools with X..Y.. hits. Anything else is ignored.
#
# Usage: awk -v color=#c83 -v opacity=0.85 -f gerber2svg.awk file.gbr  > fragment.svg
#        awk -v drill=1 -f gerber2svg.awk file.drl                      > fragment.svg
# The output is a <g> fragment; tools/render.sh wraps fragments in a full SVG document.

function num(s,   v) { v = s + 0; return v / 1000000.0 }
function emit_poly(   i, s) {
    s = ""
    for (i = 1; i <= np; i++) s = s sprintf("%.4f,%.4f ", px[i], -py[i])
    printf "<polygon points=\"%s\" fill=\"%s\"/>\n", s, color
}
function coord(line, key,   i, j, s) {
    i = index(line, key)
    if (i == 0) return "none"
    s = substr(line, i + 1)
    j = match(s, /^-?[0-9.]+/)
    if (j == 0) return "none"
    return substr(s, 1, RLENGTH)
}
BEGIN {
    if (color == "") color = "#c83"
    if (opacity == "") opacity = "0.85"
    printf "<g opacity=\"%s\">\n", opacity
    cx = 0; cy = 0; inreg = 0; np = 0; cur = ""; lines = 0; flashes = 0; regions = 0; hits = 0
}
{ gsub(/\r/, "") }
drill == 1 && /^T[0-9]+C[0-9.]+/ {
    t = $0; sub(/C.*/, "", t); d = $0; sub(/^T[0-9]+C/, "", d); tool[t] = d + 0; next
}
drill == 1 && /^T[0-9]+$/ { curtool = $0; next }
drill == 1 && /^X/ {
    x = coord($0, "X") + 0; y = coord($0, "Y") + 0
    printf "<circle cx=\"%.4f\" cy=\"%.4f\" r=\"%.4f\" fill=\"#000\" stroke=\"#fff\" stroke-width=\"0.05\"/>\n", x, -y, tool[curtool] / 2
    hits++
    next
}
drill == 1 { next }
/^%ADD/ {
    s = $0; sub(/^%ADD/, "", s)
    code = s; sub(/[A-Z].*/, "", code)
    shape = substr(s, length(code) + 1, 1)
    par = s; sub(/^[0-9]+[A-Z],/, "", par); sub(/\*%$/, "", par)
    ap_shape["D" code] = shape
    if (shape == "R") {
        split(par, wh, "X"); ap_w["D" code] = wh[1] + 0; ap_h["D" code] = wh[2] + 0
    } else { ap_w["D" code] = par + 0; ap_h["D" code] = par + 0 }
    next
}
/^G36/ { inreg = 1; np = 0; next }
/^G37/ { if (np > 2) { emit_poly(); regions++ } inreg = 0; np = 0; next }
/^D[0-9]+\*$/ { cur = $0; sub(/\*$/, "", cur); next }
/^X|^Y/ {
    xs = coord($0, "X"); ys = coord($0, "Y")
    nx = (xs == "none") ? cx : num(xs); ny = (ys == "none") ? cy : num(ys)
    op = ""
    if (index($0, "D01")) op = "1"; else if (index($0, "D02")) op = "2"; else if (index($0, "D03")) op = "3"
    if (inreg) {
        if (op == "2") np = 0
        np++; px[np] = nx; py[np] = ny
    } else if (op == "1") {
        printf "<line x1=\"%.4f\" y1=\"%.4f\" x2=\"%.4f\" y2=\"%.4f\" stroke=\"%s\" stroke-width=\"%.4f\" stroke-linecap=\"round\"/>\n", cx, -cy, nx, -ny, color, ap_w[cur]
        lines++
    } else if (op == "3") {
        if (ap_shape[cur] == "R")
            printf "<rect x=\"%.4f\" y=\"%.4f\" width=\"%.4f\" height=\"%.4f\" fill=\"%s\"/>\n", nx - ap_w[cur] / 2, -ny - ap_h[cur] / 2, ap_w[cur], ap_h[cur], color
        else
            printf "<circle cx=\"%.4f\" cy=\"%.4f\" r=\"%.4f\" fill=\"%s\"/>\n", nx, -ny, ap_w[cur] / 2, color
        flashes++
    }
    cx = nx; cy = ny
    next
}
END {
    print "</g>"
    printf "<!-- stats lines=%d flashes=%d regions=%d drill_hits=%d -->\n", lines, flashes, regions, hits
}

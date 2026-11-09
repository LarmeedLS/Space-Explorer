#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# SPACE-EXPLORER: PLANET & MOON TEXTURE CONVERTER (tmp/skin -> assets/.../*.ktx2)
# ==============================================================================
# Converts high-resolution source maps (.webp, .jpg, .png) from tmp/skin/ into
# ultra-fast GPU-compressed BC7 KTX2 containers with pre-generated mipmaps.
# ==============================================================================

SRC_DIR="tmp/skin"
OUT_DIR="assets/textures/planets"

mkdir -p "$OUT_DIR"

if ! command -v toktx &> /dev/null; then
    echo "[ERROR] 'toktx' not found. Install with: paru -S ktx-software-bin"
    exit 1
fi

if ! command -v magick &> /dev/null && ! command -v convert &> /dev/null; then
    echo "[ERROR] ImageMagick ('magick' or 'convert') not found."
    exit 1
fi

CONVERT_CMD="magick"
if ! command -v magick &> /dev/null; then
    CONVERT_CMD="convert"
fi

echo "================================================================================"
echo "          SPACE-EXPLORER: MOON & PLANET SKIN CONVERSION PIPELINE                "
echo "================================================================================"
echo "[INFO] Using: $CONVERT_CMD + toktx ($(toktx --version 2>&1 | head -n 1))"
echo "[INFO] Source directory: $SRC_DIR"
echo "[INFO] Destination directory: $OUT_DIR"
echo ""

# Define target conversions: [source_filename]="target_stem"
declare -A TARGETS=(
    # Jovian Moons
    ["Dh_io_texture.webp"]="io"
    ["Dh_europa_texture.webp"]="europa"
    ["Dh_ganymede_texture.webp"]="ganymede"
    ["Dh_callisto_texture.webp"]="callisto"

    # Martian Moons
    ["Dh_phobos_texture.webp"]="phobos"
    ["Dh_deimos_texture.webp"]="deimos"

    # Saturnian Moons
    ["Mimas.webp"]="mimas"
    ["Enceladus.webp"]="enceladus"
    ["Methone.webp"]="methone"
    ["Dione.webp"]="dione"
    ["RS3_Rhea.webp"]="rhea"
    ["Titan_map_augh.webp"]="titan"
    ["LapetusNew.webp"]="iapetus"

    # Uranian Moons
    ["MirandaNew.webp"]="miranda"
    ["ArielNew.webp"]="ariel"
    ["UmbrielNew.webp"]="umbriel"
    ["TitaniaNew.webp"]="titania"
    ["OberonNew.webp"]="oberon"

    # Neptunian Moons
    ["triton_texture_map_14k__by_askaniy_dcln18g.png"]="triton"
    ["RS3_Triton.webp"]="triton"
    ["ProteusNew.webp"]="proteus"
    ["Nereidimproved.webp"]="nereid"

    # Pluto System & Dwarf Planets / Kuiper Belt Objects
    ["PlutoColour.webp"]="pluto"
    ["Charonrealcolor.webp"]="charon"
    ["Dh_ceres_texture.webp"]="ceres"
    ["Eris2.webp"]="eris"
    ["THaumea.webp"]="haumea"
    ["Makemakemakemakeuhhhhidkidk.webp"]="makemake"
    ["Gonggong.webp"]="gonggong"
    ["Sedna_diffuse.tex.webp"]="sedna"
    ["Dh_farfarout_texture.webp"]="farfarout"
    ["TPlanet9.webp"]="planet9"
)

# By default, process Jovian moons unless specific names or --all are passed
DEFAULT_STEMS=("io" "europa" "ganymede" "callisto")
SELECTED=()

if [ "$#" -gt 0 ]; then
    if [ "$1" == "--all" ]; then
        for src in "${!TARGETS[@]}"; do
            SELECTED+=("$src")
        done
    else
        for arg in "$@"; do
            for src in "${!TARGETS[@]}"; do
                stem="${TARGETS[$src]}"
                if [[ "$stem" == "$arg" || "$src" == *"$arg"* ]]; then
                    SELECTED+=("$src")
                fi
            done
        done
    fi
else
    for def_stem in "${DEFAULT_STEMS[@]}"; do
        for src in "${!TARGETS[@]}"; do
            if [ "${TARGETS[$src]}" == "$def_stem" ]; then
                SELECTED+=("$src")
            fi
        done
    done
fi

for src_name in "${SELECTED[@]}"; do
    src_file="$SRC_DIR/$src_name"
    stem="${TARGETS[$src_name]}"
    out_file="$OUT_DIR/${stem}.ktx2"

    if [ ! -f "$src_file" ]; then
        echo "► Skipping $src_name (not found in $SRC_DIR)"
        continue
    fi

    echo "► Processing: $src_name -> ${stem}.ktx2"
    tmp_png="/tmp/temp_${stem}.png"
    
    # Check dimensions for hardware compatibility and BC7 4-texel block alignment
    w=$($CONVERT_CMD "$src_file" -format "%w" info:)
    h=$($CONVERT_CMD "$src_file" -format "%h" info:)
    if (( w > 8192 || h > 4096 )); then
        echo "  [INFO] Clamping resolution (${w}x${h} -> 8192x4096) for hardware compatibility"
        $CONVERT_CMD "$src_file" -strip -resize 8192x4096\! "$tmp_png"
    elif (( w % 4 != 0 || h % 4 != 0 )); then
        echo "  [INFO] Normalizing dimensions (${w}x${h} -> 2048x1024) for BC7 block alignment"
        $CONVERT_CMD "$src_file" -strip -resize 2048x1024\! "$tmp_png"
    else
        $CONVERT_CMD "$src_file" -strip "$tmp_png"
    fi
    
    # Encode with Khronos UASTC + Zstd level 9 + full mipmaps + sRGB transfer
    toktx --encode uastc --zcmp 9 --genmipmap --assign_oetf srgb "$out_file" "$tmp_png"
    
    rm -f "$tmp_png"
    
    orig_size=$(stat -c%s "$src_file")
    new_size=$(stat -c%s "$out_file")
    h_orig=$(numfmt --to=iec-i --suffix=B "$orig_size" 2>/dev/null || echo "${orig_size} B")
    h_new=$(numfmt --to=iec-i --suffix=B "$new_size" 2>/dev/null || echo "${new_size} B")
    echo "  [SUCCESS] Created ${stem}.ktx2 (Disk: $h_orig -> $h_new)"
done

echo ""
echo "================================================================================"
echo "                        CONVERSION COMPLETED                                   "
echo "================================================================================"

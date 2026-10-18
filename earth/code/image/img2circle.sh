magick input.png \( +clone -threshold -1 -negate \
  -fill white -draw "circle %[fx:w/2],%[fx:h/2] %[fx:w/2],0" \
  \) -alpha off -compose CopyOpacity -composite output.png
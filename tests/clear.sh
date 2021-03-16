#!/usr/bin/env zsh

delay=1

function fill_screen() {
    for (( i=0; $i + 1 < $LINES; i=1+$i )); do
        python -c "print('.' * ${COLUMNS}, end='')"
    done
}

function go_center() {
    echo -en "\e[$(( $LINES / 2 ));$(( $COLUMNS / 2 ))H"
}

clear
echo 'erase right'
fill_screen
go_center
sleep $delay
echo -en '\e[0K'
sleep $delay
echo -en '@'
sleep $delay

clear
echo 'erase left'
fill_screen
go_center
sleep $delay
echo -en '\e[1K'
sleep $delay
echo -en '@'
sleep $delay

clear
echo 'erase entire line'
fill_screen
go_center
sleep $delay
echo -en '\e[2K'
sleep $delay
echo -en '@'
sleep $delay

clear
echo 'erase bellow'
fill_screen
go_center
sleep $delay
echo -en '\e[0J'
sleep $delay
echo -en '@'
sleep $delay

clear
echo 'erase above'
fill_screen
go_center
sleep $delay
echo -en '\e[1J'
sleep $delay
echo -en '@'
sleep $delay

clear
echo 'erase entire screen'
fill_screen
go_center
sleep $delay
echo -en '\e[2J'
sleep $delay
echo -en '@'
sleep $delay

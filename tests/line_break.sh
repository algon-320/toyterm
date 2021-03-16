#!/usr/bin/env zsh

delay=1

function fill_screen() {
    for (( i=0; $i + 2 < $LINES; i=1+$i )); do
        python -c "print('.' * $(( ${COLUMNS} - 1 )), end='\n')"
    done
}

function go_center() {
    echo -en "\e[$(( $LINES / 2 ));$(( $COLUMNS / 2 ))H"
}

clear
echo "LINES=${LINES}, COLUMNS=${COLUMNS}"
echo 'line break'
echo '========'
python -c "print(('0123456789' * ${COLUMNS})[:$(( ${COLUMNS} - 1 ))], end='')"
echo -e 'あ'
echo '========'
python -c "print(('0123456789' * ${COLUMNS})[:$(( ${COLUMNS} - 2 ))], end='')"
echo -e 'あ'
echo '========'
python -c "print(('0123456789' * ${COLUMNS})[:${COLUMNS}], end='')"
echo -e '1'
echo '========'
python -c "print(('0123456789' * ${COLUMNS})[:${COLUMNS}], end='')"
echo -e '\n2'
echo '========'
python -c "print(('0123456789' * ${COLUMNS})[:${COLUMNS}], end='')"
echo -e '\n\n3'
echo '========'

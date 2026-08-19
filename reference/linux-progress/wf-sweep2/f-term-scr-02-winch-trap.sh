#!/bin/bash
trap 'printf "WINCH %s cols=%s\n" "$(date +%s.%3N)" "$(tput cols)"' WINCH
echo TRAP_READY
while true; do sleep 0.02; done

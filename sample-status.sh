# /usr/bin/env bash
# # this scripts leverages the live stystem tests fixtures (commmlete dotifles repo )
# to run a dodot status on that dir, which allows us to test and tweat the output, it's not intenede
# to run or alter data.
#
#
# make sure we have a new buiild
scripts/build &&
    # set the project root
    DOTFILES_ROOT="$PROJECT_ROOT/live-testing/scenarios/suite-3-multi-powerups-multi-packs/dotfiles"
# finally run the status command
dodot status

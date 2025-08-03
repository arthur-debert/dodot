#!/bin/zsh
# Simple hello world test to verify ShellSpec is working

Describe 'Basic ShellSpec verification'
  It 'can run a simple test'
    When call echo "Hello World"
    The output should equal "Hello World"
  End

  It 'can verify true conditions'
    When call true
    The status should be success
  End

  It 'can verify false conditions'
    When call false
    The status should be failure
  End
End

Describe 'Dodot binary verification'
  It 'dodot binary exists and is executable'
    Path executable="$DODOT"
    The path "$DODOT" should be executable
  End

  It 'dodot can show version'
    When call "$DODOT" --version
    The status should be success
    The output should include "dodot version"
  End
End
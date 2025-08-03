#!/bin/zsh
# Example of how to write dodot tests with proper verification

Describe 'Example dodot test with verification'
  BeforeEach 'reset_test_environment'
  AfterEach 'cleanup_test_environment'
  
  Describe 'running dodot deploy'
    It 'starts with verified clean environment'
      # Always verify clean state at the start of important tests
      When call verify_clean_environment
      The status should be success
    End
    
    It 'creates expected directories when running deploy'
      # First verify we're starting clean
      When call verify_clean_environment
      The status should be success
      
      # Run dodot deploy
      When call "$DODOT" deploy vim
      The status should be success
      
      # Verify dodot created its directories
      When call test -d "$HOME/.local/share/dodot"
      The status should be success
      
      # Can dump state for debugging if needed
      # dump_environment_state
    End
    
    It 'environment is clean for next test'
      # Even after previous test ran dodot, we should be clean
      When call verify_clean_environment
      The status should be success
    End
  End
  
  Describe 'debugging failed tests'
    It 'shows how to debug when verification fails'
      # Intentionally contaminate
      mkdir -p "$HOME/.local/share/dodot/mystery"
      
      # This will fail and show what's wrong
      When call verify_clean_environment
      The status should be failure
      
      # In a real test, you might want to dump state
      # to understand what happened:
      # dump_environment_state
    End
  End
End
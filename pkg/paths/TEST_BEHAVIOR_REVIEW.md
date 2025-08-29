# Test Behavior Review for pkg/paths

## Tests with Behavior Discrepancies

### 1. TestSplitPath
**Issue**: `filepath.Split()` includes the trailing slash in the directory part
- Test expects: `"/home/user/"` → dir: `"/home/user"`, file: `""`
- Actual behavior: `"/home/user/"` → dir: `"/home/user/"`, file: `""`
- **Recommendation**: Update test to match Go's standard `filepath.Split()` behavior

### 2. TestIsHiddenPath
**Issue**: The implementation uses `filepath.Base()` which has specific behavior for special cases
- `"."` → `filepath.Base()` returns `"."`, so `IsHiddenPath(".")` returns `true`
- `".."` → `filepath.Base()` returns `".."`, so `IsHiddenPath("..")` returns `true`
- `""` → `filepath.Base()` returns `"."`, so `IsHiddenPath("")` returns `true`
- `"/home/.config/vim/vimrc"` → `filepath.Base()` returns `"vimrc"`, so `IsHiddenPath()` returns `false`

**Test expectations**:
- `.` and `..` should return `false` (they're not hidden files)
- Empty path should return `false`
- Path with hidden directory in middle should return `true`

**Recommendation**: The implementation needs clarification on requirements:
1. Should it check if ANY component is hidden? 
2. Should it only check the final component?
3. Should `.` and `..` be considered hidden?

### 3. TestPathDepth  
**Issue**: The implementation counts differently than expected
- Test expects: `"/home"` → depth 1
- Actual might be: `"/home"` → depth 0 (counting directories from root)

**Recommendation**: Review the PathDepth implementation to understand its counting logic

### 4. TestCommonPrefix
**Issue**: When paths share only the root `/`, the implementation returns `/` instead of empty string
- Test expects: `["/home", "/usr", "/var"]` → `""`
- Actual behavior: `["/home", "/usr", "/var"]` → `"/"`

**Recommendation**: Decide if a shared root should count as a common prefix

### 5. TestNew (from paths_test.go)
**Issue**: The `paths.New()` function doesn't validate if the dotfiles root exists or is a directory
- Test expects error for non-existent path
- Test expects error for file (not directory)
- Actual: No validation, accepts any path

**Recommendation**: Either:
1. Add validation to `paths.New()` 
2. Update tests to not expect validation

### 6. TestPathsGetters (from paths_test.go)
**Issues**: Several path calculations don't match expectations
- `ShellProfileDir()`: Returns `.../deployed/shell` not `.../deployed/shell_profiles`
- `ShellDir()`: Returns `.../shell` not `.../deployed/shell`
- `InitScriptPath()`: Returns `.../shell/dodot-init.sh` not `.../deployed/init.sh`
- `ProvisionDir()`: Returns `.../provision` not `.../deployed/provision`
- `HomebrewDir()`: Returns `.../homebrew` not `.../deployed/homebrew`

**Recommendation**: Review the actual directory structure design

## Tests That Are Correct

The following tests pass and represent correct behavior:
- TestValidatePath
- TestValidatePackName  
- TestSanitizePath
- TestIsAbsolutePath
- TestJoinPaths
- TestContainsPath
- TestValidatePathSecurity
- TestMustValidatePath
- TestRelativePath
- TestExpandHome
- TestResolveShellScriptPath
- TestGetShellScriptPath
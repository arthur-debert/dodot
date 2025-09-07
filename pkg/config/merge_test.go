package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestMergeMaps(t *testing.T) {
	tests := []struct {
		name     string
		dest     map[string]interface{}
		src      map[string]interface{}
		expected map[string]interface{}
	}{
		{
			name: "simple_values_overwrite",
			dest: map[string]interface{}{
				"key1": "value1",
				"key2": "value2",
			},
			src: map[string]interface{}{
				"key2": "new_value2",
				"key3": "value3",
			},
			expected: map[string]interface{}{
				"key1": "value1",
				"key2": "new_value2",
				"key3": "value3",
			},
		},
		{
			name: "nested_maps_merge",
			dest: map[string]interface{}{
				"outer": map[string]interface{}{
					"inner1": "value1",
					"inner2": "value2",
				},
			},
			src: map[string]interface{}{
				"outer": map[string]interface{}{
					"inner2": "new_value2",
					"inner3": "value3",
				},
			},
			expected: map[string]interface{}{
				"outer": map[string]interface{}{
					"inner1": "value1",
					"inner2": "new_value2",
					"inner3": "value3",
				},
			},
		},
		{
			name: "slices_append",
			dest: map[string]interface{}{
				"list": []interface{}{"item1", "item2"},
			},
			src: map[string]interface{}{
				"list": []interface{}{"item3", "item4"},
			},
			expected: map[string]interface{}{
				"list": []interface{}{"item1", "item2", "item3", "item4"},
			},
		},
		{
			name: "string_slices_append",
			dest: map[string]interface{}{
				"patterns": []string{".git", "node_modules"},
			},
			src: map[string]interface{}{
				"patterns": []string{"*.tmp", "backup"},
			},
			expected: map[string]interface{}{
				"patterns": []interface{}{".git", "node_modules", "*.tmp", "backup"},
			},
		},
		{
			name: "mixed_slice_types_append",
			dest: map[string]interface{}{
				"list": []string{"str1", "str2"},
			},
			src: map[string]interface{}{
				"list": []interface{}{"str3", "str4"},
			},
			expected: map[string]interface{}{
				"list": []interface{}{"str1", "str2", "str3", "str4"},
			},
		},
		{
			name: "deep_nested_with_arrays",
			dest: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{".git", ".svn"},
				},
			},
			src: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{"custom-ignore", "temp-*"},
				},
			},
			expected: map[string]interface{}{
				"patterns": map[string]interface{}{
					"pack_ignore": []interface{}{".git", ".svn", "custom-ignore", "temp-*"},
				},
			},
		},
		{
			name: "slices_deduplicate",
			dest: map[string]interface{}{
				"shell": []interface{}{"profile.sh", "aliases.sh"},
			},
			src: map[string]interface{}{
				"shell": []interface{}{"aliases.sh", "functions.sh"},
			},
			expected: map[string]interface{}{
				"shell": []interface{}{"profile.sh", "aliases.sh", "functions.sh"},
			},
		},
		{
			name: "overwrite_non_slice_with_slice",
			dest: map[string]interface{}{
				"value": "string",
			},
			src: map[string]interface{}{
				"value": []string{"item1", "item2"},
			},
			expected: map[string]interface{}{
				"value": []string{"item1", "item2"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Make a copy of dest since mergeMaps modifies it in place
			result := make(map[string]interface{})
			for k, v := range tt.dest {
				result[k] = v
			}

			mergeMaps(result, tt.src)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestIsSlice(t *testing.T) {
	tests := []struct {
		name     string
		value    interface{}
		expected bool
	}{
		{
			name:     "interface_slice",
			value:    []interface{}{"a", "b"},
			expected: true,
		},
		{
			name:     "string_slice",
			value:    []string{"a", "b"},
			expected: true,
		},
		{
			name:     "string",
			value:    "not a slice",
			expected: false,
		},
		{
			name:     "int",
			value:    42,
			expected: false,
		},
		{
			name:     "map",
			value:    map[string]interface{}{"key": "value"},
			expected: false,
		},
		{
			name:     "nil",
			value:    nil,
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := isSlice(tt.value)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestAppendSlices(t *testing.T) {
	tests := []struct {
		name     string
		dest     interface{}
		src      interface{}
		expected []interface{}
	}{
		{
			name:     "interface_slices",
			dest:     []interface{}{"a", "b"},
			src:      []interface{}{"c", "d"},
			expected: []interface{}{"a", "b", "c", "d"},
		},
		{
			name:     "string_slices",
			dest:     []string{"a", "b"},
			src:      []string{"c", "d"},
			expected: []interface{}{"a", "b", "c", "d"},
		},
		{
			name:     "mixed_slice_types",
			dest:     []string{"a", "b"},
			src:      []interface{}{"c", "d"},
			expected: []interface{}{"a", "b", "c", "d"},
		},
		{
			name:     "empty_dest",
			dest:     []interface{}{},
			src:      []string{"a", "b"},
			expected: []interface{}{"a", "b"},
		},
		{
			name:     "empty_src",
			dest:     []string{"a", "b"},
			src:      []interface{}{},
			expected: []interface{}{"a", "b"},
		},
		{
			name:     "deduplicate_strings",
			dest:     []string{"a", "b", "c"},
			src:      []string{"b", "d", "a", "e"},
			expected: []interface{}{"a", "b", "c", "d", "e"},
		},
		{
			name:     "preserve_order_with_dedup",
			dest:     []interface{}{"profile.sh", "aliases.sh"},
			src:      []interface{}{"aliases.sh", "functions.sh", "profile.sh"},
			expected: []interface{}{"profile.sh", "aliases.sh", "functions.sh"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := appendSlices(tt.dest, tt.src)
			assert.Equal(t, tt.expected, result)
		})
	}
}

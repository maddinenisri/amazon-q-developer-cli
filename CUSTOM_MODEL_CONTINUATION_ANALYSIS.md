# Custom Model Continuation Prompt Issue Analysis

## Executive Summary

The custom model implementation successfully executes tools and maintains tool result feedback, but encounters a continuation prompt issue in non-interactive mode that causes repetitive execution of the same task instead of progressing through multi-step workflows. This document provides a comprehensive analysis of the problem, affected components, and testing procedures.

## Problem Description

### Core Issue
When custom models are used in non-interactive mode for multi-step tasks, the system sends generic continuation prompts ("Please continue with the next steps or confirm if the task is complete.") that lack sufficient context about completed steps. This causes the model to repeat the same task multiple times instead of progressing to subsequent steps.

### Manifestation
- **Expected Behavior**: Create multiple files (e.g., `file1.txt`, `file2.txt`, `file3.txt`) in sequence
- **Actual Behavior**: Creates the first file repeatedly, never progressing to subsequent files
- **Tool Execution**: Tools execute successfully, but context about completion is not properly conveyed

### Root Cause Analysis

1. **Insufficient Context in Continuation Prompts**: The current implementation sends generic continuation messages without detailed task completion status
2. **Tool Result Processing Gap**: While tool results are captured in the request (`toolResults` field), the model doesn't receive clear indication of which specific tasks have been completed
3. **Conversation State Management**: The conversation history doesn't provide enough granular information about multi-step task progress

## Technical Analysis

### Current Implementation Status

#### ✅ Working Components
- **SSE Parsing**: Fixed to handle `content_block_stop` events correctly
- **Tool Execution**: Local tool execution through existing infrastructure  
- **Tool Result Feedback**: Tool results included in `CustomModelRequest.tool_results`
- **Single-Step Tasks**: Work perfectly for individual file creation

#### ❌ Problematic Components  
- **Continuation Prompt Generation**: Generic prompts lack task-specific context
- **Multi-Step Task Tracking**: No mechanism to track which steps in a sequence are complete
- **Non-Interactive Mode Flow**: Doesn't provide sufficient context for complex workflows

### File Analysis

#### Primary Files Involved

**`/crates/chat-cli/src/api_client/custom_model.rs`**
- **Lines 605-610**: Generic continuation prompt generation
```rust
let message_content = if conversation.user_input_message.content.trim().is_empty() {
    debug!("Current message is empty, using default continuation prompt");
    "Please continue with the next steps or confirm if the task is complete.".to_string()
} else {
    conversation.user_input_message.content
};
```
- **Impact**: This generic message doesn't provide context about completed vs. remaining tasks
- **Tool Results Integration**: Lines 620-625 correctly extract tool results but don't use them for context

**`/crates/chat-cli/src/api_client/model.rs`**
- **Lines 427-494**: Tool result serialization implementation
- **Purpose**: Enables tool results to be included in custom model requests
- **Status**: ✅ Working correctly

**`/crates/chat-cli/src/cli/chat/mod.rs`**
- **Tool Execution Pipeline**: Lines ~1464+ handle tool execution and result generation
- **Integration Point**: Where tool results are generated and could be enhanced for better context

#### Request/Response Flow Files

**`custom_model_request.json`** (Generated during execution)
- **Contains**: Complete request sent to custom model proxy
- **Key Fields**:
  - `message`: The continuation prompt (currently generic)
  - `toolResults`: Array of completed tool executions with status
  - `history`: Conversation history
- **Analysis Point**: Shows tool results are captured but not used for prompt enhancement

### Comparison with Built-in Models

Built-in Amazon Q models work correctly because:
1. **Direct Integration**: They have direct access to tool execution context
2. **Sophisticated Continuation**: Internal logic understands task completion state
3. **Conversation Memory**: Better integration with conversation state management

Custom models need enhanced continuation logic to match this behavior.

## Proposed Solution Architecture

### Solution Approach
Enhance the continuation prompt generation to include specific context about completed tool executions and remaining tasks.

### Implementation Strategy

#### 1. Enhanced Continuation Prompt Generation
Create intelligent continuation prompts that include:
- Summary of completed tool executions
- Explicit list of remaining tasks
- Clear indication of progress through multi-step workflows

#### 2. Tool Result Context Integration
Modify the continuation prompt generation to analyze `tool_results` and provide specific feedback:

```rust
// Proposed enhancement in custom_model.rs
fn generate_continuation_prompt(
    conversation: &ConversationState,
    tool_results: &Option<Vec<ToolResult>>
) -> String {
    if let Some(results) = tool_results {
        let completed_actions = results.iter()
            .filter(|r| r.status == ToolResultStatus::Success)
            .collect::<Vec<_>>();
        
        if !completed_actions.is_empty() {
            return format!(
                "I have completed {} actions successfully. Please continue with the next steps or confirm if the task is complete. Previous completions: {}",
                completed_actions.len(),
                // Include summary of completed actions
            );
        }
    }
    
    "Please continue with the next steps or confirm if the task is complete.".to_string()
}
```

#### 3. Multi-Step Task Tracking
Implement task progress tracking that can:
- Parse multi-step requests into discrete tasks
- Track completion status of each task
- Generate context-aware continuation prompts

## Testing Procedures

### Test Environment Setup

```bash
# Build production version
cargo build --release -p chat_cli

# Clean previous test files
rm -f test_*.txt custom_*.txt production_*.txt custom_model_*.json
```

### Test Cases

#### Test Case 1: Multi-Step File Creation
**Purpose**: Verify multi-step task progression
```bash
./target/release/chat_cli chat --model custom:test-model --non-interactive --trust-all-tools \
  "Create three files: test_step1.txt with content 'Step 1 Complete', test_step2.txt with content 'Step 2 Complete', and test_step3.txt with content 'Step 3 Complete'"
```

**Expected Results**:
```bash
ls test_step*.txt
# Should show: test_step1.txt test_step2.txt test_step3.txt

cat test_step1.txt test_step2.txt test_step3.txt
# Should show:
# Step 1 Complete
# Step 2 Complete  
# Step 3 Complete
```

**Current Behavior**: Creates only `test_step1.txt` repeatedly

#### Test Case 2: Mixed Task Types
**Purpose**: Test different tool combinations
```bash
./target/release/chat_cli chat --model custom:test-model --non-interactive --trust-all-tools \
  "First create a file named mixed_test.txt with content 'Created', then read the file to confirm, then create another file named mixed_test2.txt with content 'Second file'"
```

#### Test Case 3: Single Task Verification
**Purpose**: Confirm single tasks work correctly
```bash
./target/release/chat_cli chat --model custom:test-model --non-interactive --trust-all-tools \
  "Create a single file named single_test.txt with content 'Single task works'"
```

**Expected**: ✅ Should work correctly

### Debugging Commands

#### Monitor Request Generation
```bash
# Watch the request JSON being generated
tail -f custom_model_request.json
```

#### Analyze Tool Results
```bash
# Check tool result structure
jq '.toolResults' custom_model_request.json
```

#### Compare with Built-in Models
```bash
# Test same task with built-in model
./target/release/chat_cli chat --model claude-3.5-sonnet --non-interactive --trust-all-tools \
  "Create three files: builtin_step1.txt, builtin_step2.txt, builtin_step3.txt"
```

### Log Analysis

#### Enable Debug Logging
```bash
RUST_LOG=debug ./target/release/chat_cli chat --model custom:test-model --non-interactive --trust-all-tools "multi-step task"
```

#### Key Log Patterns to Monitor
- `"Current message is empty, using default continuation prompt"`
- Tool execution completion messages
- Custom model request generation
- SSE event processing

## Impact Assessment

### Severity: **High**
- **Functionality**: Multi-step tasks completely non-functional in non-interactive mode
- **User Experience**: Confusing behavior with repeated operations
- **Production Readiness**: Blocks production deployment for complex workflows

### Affected Use Cases
1. **Batch File Operations**: Creating multiple files in sequence
2. **Complex Workflows**: Multi-step development tasks
3. **Automation Scripts**: Non-interactive scripted operations
4. **CI/CD Integration**: Automated development workflows

### Working Scenarios
1. **Single-Step Tasks**: ✅ Work perfectly
2. **Interactive Mode**: ✅ Users can manually guide progression
3. **Built-in Models**: ✅ No issues with multi-step tasks

## Success Criteria

### Functional Requirements
1. **Multi-Step Progression**: Custom models must progress through all steps in sequence
2. **Tool Result Awareness**: Models must understand what has been completed
3. **Context Preservation**: Continuation prompts must include relevant completion status
4. **Parity with Built-in Models**: Similar behavior to Amazon Q built-in models

### Technical Validation
1. **Test Case 1**: All three files created with correct content
2. **Request Analysis**: Continuation prompts include completion context
3. **Performance**: No significant performance degradation
4. **Compatibility**: Works with existing custom model proxy servers

## Implementation Priority

### Phase 1: Quick Fix (Immediate)
- Enhance continuation prompt to include tool result summary
- Provide basic completion context

### Phase 2: Comprehensive Solution (Short-term)
- Implement proper multi-step task tracking
- Add sophisticated continuation logic
- Enhance conversation state management

### Phase 3: Advanced Features (Long-term)
- Task dependency management
- Rollback capabilities for failed multi-step operations
- Advanced workflow orchestration

## Related Issues and Dependencies

### Upstream Dependencies
- **Custom Model Proxy**: Must support enhanced request format
- **Tool Infrastructure**: Current implementation sufficient
- **Conversation Management**: May need enhancements for complex workflows

### Downstream Impact
- **Documentation**: Update custom model integration guides
- **Testing**: Expand test coverage for multi-step scenarios
- **Examples**: Provide multi-step workflow examples

## Conclusion

The continuation prompt issue represents a critical gap in custom model functionality that prevents effective multi-step task execution. While the underlying tool execution and feedback mechanisms work correctly, the lack of intelligent continuation prompts creates a poor user experience and limits the practical utility of custom models in automation scenarios.

The proposed solution is well-scoped and builds on the existing working infrastructure, making it a high-impact, moderate-effort enhancement that will significantly improve custom model capability and user satisfaction.
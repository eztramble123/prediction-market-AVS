# Prediction Market AVS 
Introduction:
As a part of the Infinite Hacker at Devcon Bangkok our team of 4 Blockchain at Berkeley members hacked from Nov 11 - Nov 15. This research document details our overarching idea, our progress, and our experience working with Layer and Eigenlayer.


# Intended WorkFlow

```mermaid
sequenceDiagram
    participant Event_Source
    participant AVS_Contract
    participant Operator

    Event_Source ->> AVS_Contract: Emit NewTaskCreated event
    AVS_Contract ->> AVS_Contract: Add task to queue
    loop For each task in the queue
        AVS_Contract ->> AVS_Contract: Process next task
        loop For each registered and staked Operator
            AVS_Contract -->> Operator: Notify of NewTaskCreated event
            Operator ->> Operator: Observe off-chain event
            Operator ->> Operator: Generate result, hash it, and sign the hash
            Operator ->> AVS_Contract: Submit signed hash
            AVS_Contract ->> AVS_Contract: Verify Operator's registration and stake
            AVS_Contract ->> Operator: Accept submission if valid
        end
        AVS_Contract ->> AVS_Contract: Tally votes with weightage
        alt Majority consensus achieved
            AVS_Contract ->> AVS_Contract: Determine truth based on majority
            AVS_Contract ->> Operator: Slash operators with minority votes
        else No consensus
            AVS_Contract ->> AVS_Contract: Mark task as unresolved
        end
        AVS_Contract ->> AVS_Contract: Remove task from queue
    end


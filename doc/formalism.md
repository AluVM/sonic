Stash is an ordered set of operations under a contract, such that for any given unordered subset of
operations which represent part of some terminal contract state it can produce an evaluation order.

Let $C$ be an unordered set of all known contract operations; $S$ to represent a set of all valid
contract states which can be reconstructed from $C$. Then, a stash is an ordered
set $A \subseteq C$, such that

$$
\forall X \subseteq C | \mathtt{eval}(X) \in S : \mathtt{eval}(A, T) \in S
$$

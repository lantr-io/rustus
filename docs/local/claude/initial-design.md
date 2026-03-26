
 We want to create frontend of scalus compiler (available at  /home/rssh/packages/nau/scalus/) to rust.

Idea is to have derive macro on enums to produce fromData/toData encodomg and SIRType in the 

Have derive macro on trait impl or on functions (what is possible) to produce SIR

Due to limitation of a rust macro system this will be a two-phase operation

On first phase, procedural macro will parse TokenStream via sin and create a pre-SIR, where 
 we have use statements and untyped SIR  (or may-be program to generate pre-SIR) and register PRE-SIR
in a global dictionary

On the second phase - process wat was added in registry and receive scalus SIR

then add in scalus loading of serialising SIR from file

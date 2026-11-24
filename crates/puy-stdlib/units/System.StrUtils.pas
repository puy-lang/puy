unit System.StrUtils;

// Routines on strings (docs/spec/stdlib/System.StrUtils/README.md): comparisons of the head, the
// tail and the inside of a string, searches in a list of strings, replacing, cutting, repeating,
// reversing and splitting. Written from the specification only, not from any other
// implementation. A routine that does what a member of TStringHelper does calls that member
// (docs/spec/stdlib/System/TStringHelper.md), so that the two follow one rule. IfThen is in
// System.SysUtils.
//
// Split as System.Generics.Collections (docs/spec/collections.md 1.2): Name.Interface.inc holds
// the declarations and Name.Implementation.inc the bodies of one group of routines, read only
// when a program names one of them. Search is the comparisons and the searches; Edit the
// routines that make a new string or an array of strings.

interface

uses
  System.SysUtils;

type
  // The result of SplitString (declared in System.Types in Delphi)
  TStringDynArray = TArray<string>;

{$I System.StrUtils/Search.Interface.inc}

{$I System.StrUtils/Edit.Interface.inc}

implementation

{$I System.StrUtils/Search.Implementation.inc}

{$I System.StrUtils/Edit.Implementation.inc}

end.

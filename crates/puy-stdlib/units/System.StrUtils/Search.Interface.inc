// The comparisons of the head, the tail and the inside of a string, and the searches in a list
// of strings. The *Str routines tell upper and lower case apart; the *Text routines fold both
// strings to lower case (AnsiSameText's rule). The Ansi* names are the same routines.

// Whether AText starts with, ends with or contains ASubText. Every string starts and ends with
// ''; no string contains ''.
function StartsStr(const ASubText, AText: string): Boolean;
function StartsText(const ASubText, AText: string): Boolean;
function EndsStr(const ASubText, AText: string): Boolean;
function EndsText(const ASubText, AText: string): Boolean;
function ContainsStr(const AText, ASubText: string): Boolean;
function ContainsText(const AText, ASubText: string): Boolean;
function AnsiStartsStr(const ASubText, AText: string): Boolean;
function AnsiStartsText(const ASubText, AText: string): Boolean;
function AnsiEndsStr(const ASubText, AText: string): Boolean;
function AnsiEndsText(const ASubText, AText: string): Boolean;
function AnsiContainsStr(const AText, ASubText: string): Boolean;
function AnsiContainsText(const AText, ASubText: string): Boolean;

// The position (from 1) of the first SubStr at or after the position Offset of S, or 0.
function PosEx(const SubStr, S: string; Offset: Integer = 1): Integer;

// The index (from 0) of the first string of AValues equal to AText, or -1; and whether there is
// one.
function IndexStr(const AText: string; const AValues: array of string): Integer;
function IndexText(const AText: string; const AValues: array of string): Integer;
function MatchStr(const AText: string; const AValues: array of string): Boolean;
function MatchText(const AText: string; const AValues: array of string): Boolean;
function AnsiIndexStr(const AText: string; const AValues: array of string): Integer;
function AnsiIndexText(const AText: string; const AValues: array of string): Integer;
function AnsiMatchStr(const AText: string; const AValues: array of string): Boolean;
function AnsiMatchText(const AText: string; const AValues: array of string): Boolean;

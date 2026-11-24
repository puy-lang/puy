// The routines that make a new string or an array of strings. Positions and counts outside the
// string are cut to it, as Copy does; none of these raises for them.

// AText with every AFromText replaced by AToText (ReplaceText ignores case, as AnsiSameText).
function ReplaceStr(const AText, AFromText, AToText: string): string;
function ReplaceText(const AText, AFromText, AToText: string): string;
function AnsiReplaceStr(const AText, AFromText, AToText: string): string;
function AnsiReplaceText(const AText, AFromText, AToText: string): string;

// The first ACount characters, the last ACount characters, and ACount characters from the
// position AStart (from 1) of AText.
function LeftStr(const AText: string; const ACount: Integer): string;
function RightStr(const AText: string; const ACount: Integer): string;
function MidStr(const AText: string; const AStart, ACount: Integer): string;
function AnsiLeftStr(const AText: string; const ACount: Integer): string;
function AnsiRightStr(const AText: string; const ACount: Integer): string;
function AnsiMidStr(const AText: string; const AStart, ACount: Integer): string;

// AText backwards; a surrogate pair stays a pair.
function ReverseString(const AText: string): string;
function AnsiReverseString(const AText: string): string;

// AText ACount times.
function DupeString(const AText: string; ACount: Integer): string;

// AText with the ALength characters from the position AStart (from 1) replaced by ASubText.
function StuffString(const AText: string; AStart, ALength: Integer; const ASubText: string): string;

// The parts of S between the characters of Delimiters, empty ones included.
function SplitString(const S, Delimiters: string): TStringDynArray;

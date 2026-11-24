// IfThen (docs/spec/stdlib/System.SysUtils/IfThen.md): ATrue if AValue, else AFalse. Both
// arguments are evaluated. One overload set for the numbers (System.Math in Delphi) and the
// strings (System.StrUtils in Delphi), so that a program that uses both units sees one IfThen.

function IfThen(AValue: Boolean; const ATrue: Integer; const AFalse: Integer = 0): Integer; overload;
function IfThen(AValue: Boolean; const ATrue: Int64; const AFalse: Int64 = 0): Int64; overload;
function IfThen(AValue: Boolean; const ATrue: Double; const AFalse: Double = 0.0): Double; overload;
function IfThen(AValue: Boolean; const ATrue: string; const AFalse: string = ''): string; overload;

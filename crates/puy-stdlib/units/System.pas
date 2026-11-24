unit System;

// The part of System written in Pascal: the members of the standard type helpers besides the
// built-in ToString and Length (docs/spec/objects.md 10.7), and the types every file sees
// (TInstant, TTimeSpan, TValue) and the memory APIs. The rest of the unit
// is built into the compiler. A helper declared here with the name of a built-in helper gives that helper
// its members; the other interface declarations are visible without a `uses`. The compiler reads
// the files of a part only when a file asks for one of its names (docs/spec/stdlib/System/README.md).
//
// The routines of System.SysUtils that are built into the compiler (Trim, AnsiUpperCase, ...)
// are visible here without a `uses`; the Pascal part of System.SysUtils is not.

interface

{$I System/IntegerHelpers.Interface.inc}
{$I System/FloatHelpers.Interface.inc}
{$I System/TBooleanHelper.Interface.inc}
{$I System/TCharHelper.Interface.inc}
{$I System/TStringHelper.Interface.inc}
{$I System/TTimeSpan.Interface.inc}
{$I System/TInstant.Interface.inc}
{$I System/TValue.Interface.inc}
{$I System/MemoryManagerState.Interface.inc}
{$I System/ReportMemoryLeaksOnShutdown.Interface.inc}

implementation

{$I System/MemoryManagerState.Implementation.inc}
{$I System/IntegerHelpers.Implementation.inc}
{$I System/FloatHelpers.Implementation.inc}
{$I System/TBooleanHelper.Implementation.inc}
{$I System/TCharHelper.Implementation.inc}
{$I System/TStringHelper.Implementation.inc}
{$I System/TTimeSpan.Implementation.inc}
{$I System/TInstant.Implementation.inc}
{$I System/TValue.Implementation.inc}

{$I System/ReportMemoryLeaksOnShutdown.Finalization.inc}

end.

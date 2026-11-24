program SysUtilsEnumLowHigh;

// Low / High / Default of the enumerated types built into System.SysUtils (TReplaceFlag,
// TCaseMapping, TCaseFold, TIOErrorKind; spec/types.md chapter 7): the first and last values,
// as for an enumerated type declared in Pascal, also with the name qualified with the unit.

uses
  System.SysUtils;

type
  TMine = (mA, mB, mC);

procedure Shadowed;
type
  // A type declared here hides the one of System.SysUtils
  TCaseFold = (sX, sY, sZ);
begin
  WriteLn(Low(TCaseFold), ' ', High(TCaseFold));
end;

var
  K: TIOErrorKind;
  F: TReplaceFlag;
begin
  WriteLn(Low(TMine), ' ', High(TMine));
  WriteLn(Low(TReplaceFlag), ' ', High(TReplaceFlag));
  WriteLn(Low(TCaseMapping), ' ', High(TCaseMapping));
  WriteLn(Low(TCaseFold), ' ', High(TCaseFold));
  WriteLn(Low(TIOErrorKind), ' ', High(TIOErrorKind));
  WriteLn(Ord(Low(TIOErrorKind)), ' ', Ord(High(TIOErrorKind)));
  WriteLn(Low(TReplaceFlag) = TReplaceFlag.rfReplaceAll, ' ', High(TCaseFold) = TCaseFold.cfUpper);
  WriteLn(Low(System.SysUtils.TCaseMapping), ' ', High(System.SysUtils.TIOErrorKind));
  WriteLn(Default(TIOErrorKind), ' ', Default(TReplaceFlag));
  for K := Low(TIOErrorKind) to High(TIOErrorKind) do
    Write(Ord(K), ' ');
  WriteLn;
  for F := Low(F) to High(F) do
    Write(F, ' ');
  WriteLn;
  Shadowed;
end.

program DistinctHelpers;

// Helpers and distinct types (types.md 1.3, objects.md 10.4): a value of a distinct type finds
// its own helpers first, then those of its original type, of that one's original type, … and of
// the root (the built-in ones included). A member of the distinct type's own helper with the
// name of a member of its original type's helper is not E0753.

uses System.SysUtils;

type
  TUserId = type Int64;
  TAdminId = type TUserId;
  THtml = type string;
  TSmall = type Byte;
  TTiny = type TSmall;

  TInt64Helper = record helper for Int64
    function Describe: string;
    function Twice: Int64;
  end;

  TUserIdHelper = record helper for TUserId
    function Describe: string;
  end;

  THtmlHelper = record helper for THtml
    function Wrap: THtml;
  end;

function TInt64Helper.Describe: string;
begin
  Result := 'Int64 ' + IntToStr(Self);
end;

function TInt64Helper.Twice: Int64;
begin
  Result := Self * 2;
end;

function TUserIdHelper.Describe: string;
begin
  Result := 'user ' + IntToStr(Self);
end;

function THtmlHelper.Wrap: THtml;
begin
  Result := '<p>' + Self + '</p>';
end;

var
  N: Int64;
  Id: TUserId;
  Admin: TAdminId;
  H: THtml;
  S: string;
  T: TTiny;
begin
  N := 1;
  Id := 2;
  Admin := 3;
  // Own helper first, then the original type's
  WriteLn(N.Describe, ', ', Id.Describe, ', ', Admin.Describe);
  WriteLn(Id.Twice, ' ', Admin.Twice);
  // The built-in helpers of the root
  T := 7;
  WriteLn(T.ToString, ' ', (T + 1).ToString);
  H := 'text';
  WriteLn(H.Wrap, ' ', H.Length, ' ', H.Wrap.Length);
  // A helper of the distinct type is not one of its root
  S := 'plain';
  WriteLn(S.Length);
  // `H.M(X)` with a value of a distinct type made from the helper's type
  WriteLn(TInt64Helper.Describe(Id));
end.
